import logging
import os
import re
import json
from datetime import datetime, timezone
from logging.handlers import RotatingFileHandler
from time import perf_counter
from functools import lru_cache
from typing import Optional
from crawl4ai import AsyncWebCrawler, BrowserConfig, CacheMode, CrawlerRunConfig
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, HttpUrl, Field


def bool_env(key: str, default: str = "true") -> bool:
    return os.getenv(key, default).lower() in {"1", "true", "yes", "on"}


def int_env(key: str, default: int) -> int:
    try:
        return int(os.getenv(key, str(default)))
    except ValueError:
        return default


def str_env(key: str, default: str) -> str:
    value = os.getenv(key)
    if value is None:
        return default
    value = value.strip()
    return value or default


def optional_env(key: str) -> Optional[str]:
    value = os.getenv(key)
    if value is None:
        return None
    value = value.strip()
    return value or None


class CrawlRequest(BaseModel):
    url: HttpUrl
    normalized_key: str = Field(..., min_length=1)
    max_html_bytes: int = Field(524_288, ge=4_096, le=2_097_152)
    max_text_chars: int = Field(4_000, ge=512, le=40_000)


class CrawlResponse(BaseModel):
    normalized_key: str
    url: HttpUrl
    status: str
    markdown_text: str
    cleaned_text: str
    content_type: str
    language: Optional[str]
    title: Optional[str]
    status_code: Optional[int]
    metadata: dict


@lru_cache(maxsize=1)
def browser_config() -> BrowserConfig:
    user_agent = str_env(
        "CRAWL4AI_USER_AGENT",
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
    )
    accept_language = str_env("CRAWL4AI_ACCEPT_LANGUAGE", "en-US,en;q=0.9")
    return BrowserConfig(
        headless=bool_env("CRAWL4AI_HEADLESS", "true"),
        browser_type=os.getenv("CRAWL4AI_BROWSER", "chromium"),
        user_agent=user_agent,
        viewport_width=int_env("CRAWL4AI_VIEWPORT_WIDTH", 1512),
        viewport_height=int_env("CRAWL4AI_VIEWPORT_HEIGHT", 982),
        headers={"Accept-Language": accept_language},
        enable_stealth=bool_env("CRAWL4AI_ENABLE_STEALTH", "true"),
        verbose=bool_env("CRAWL4AI_VERBOSE", "false"),
    )


def crawler_run_config() -> CrawlerRunConfig:
    return CrawlerRunConfig(
        cache_mode=CacheMode.BYPASS,
        scan_full_page=True,
        process_iframes=True,
        wait_until="networkidle",
        delay_before_return_html=0.2,
        locale=str_env("CRAWL4AI_LOCALE", "en-US"),
        timezone_id=optional_env("CRAWL4AI_TIMEZONE"),
        simulate_user=bool_env("CRAWL4AI_SIMULATE_USER", "true"),
        override_navigator=bool_env("CRAWL4AI_OVERRIDE_NAVIGATOR", "true"),
    )


app = FastAPI(title="Crawl4AI Service", version="0.1.0")
logger = logging.getLogger("crawl4ai-service")
audit_logger = logging.getLogger("crawl4ai-audit")

DEFAULT_LOG_DIR = "logs"
DEFAULT_LOG_SUBDIR = "crawl4ai"
DEFAULT_APP_LOG_FILE = "crawl4ai-service.log"
DEFAULT_AUDIT_LOG_FILE = "crawl-audit.jsonl"
DEFAULT_LOG_MAX_BYTES = 20 * 1024 * 1024
DEFAULT_LOG_BACKUP_COUNT = 10


class CrawlOutcomeError(Exception):
    def __init__(
        self,
        report: str,
        reason: str,
        detail: str,
        status_code: Optional[int],
        duration_ms: int,
    ):
        super().__init__(detail)
        self.report = report
        self.reason = reason
        self.detail = detail
        self.status_code = status_code
        self.duration_ms = duration_ms


def configure_file_logging() -> None:
    root = str_env("OD_LOG_DIR", DEFAULT_LOG_DIR)
    subdir = str_env("CRAWL4AI_LOG_SUBDIR", DEFAULT_LOG_SUBDIR)
    app_log_file = str_env("CRAWL4AI_APP_LOG_FILE", DEFAULT_APP_LOG_FILE)
    audit_log_file = str_env("CRAWL4AI_AUDIT_LOG_FILE", DEFAULT_AUDIT_LOG_FILE)
    max_bytes = int_env("CRAWL4AI_LOG_MAX_BYTES", DEFAULT_LOG_MAX_BYTES)
    backup_count = int_env("CRAWL4AI_LOG_BACKUP_COUNT", DEFAULT_LOG_BACKUP_COUNT)

    log_dir = os.path.join(root, subdir)
    os.makedirs(log_dir, exist_ok=True)

    if not any(isinstance(handler, RotatingFileHandler) for handler in logger.handlers):
        app_handler = RotatingFileHandler(
            os.path.join(log_dir, app_log_file),
            maxBytes=max_bytes,
            backupCount=backup_count,
            encoding="utf-8",
        )
        app_handler.setFormatter(
            logging.Formatter("%(asctime)s %(levelname)s %(name)s %(message)s")
        )
        logger.addHandler(app_handler)

    if not any(
        isinstance(handler, RotatingFileHandler) for handler in audit_logger.handlers
    ):
        audit_handler = RotatingFileHandler(
            os.path.join(log_dir, audit_log_file),
            maxBytes=max_bytes,
            backupCount=backup_count,
            encoding="utf-8",
        )
        audit_handler.setFormatter(logging.Formatter("%(message)s"))
        audit_logger.addHandler(audit_handler)
    audit_logger.propagate = False
    logger.setLevel(logging.INFO)
    audit_logger.setLevel(logging.INFO)


def classify_report(detail: str, status_code: Optional[int]) -> tuple[str, str]:
    text = (detail or "").lower()
    if status_code == 403:
        return "blocked", "http_403"
    blocked_markers = [
        "blocked by anti-bot",
        "access denied",
        "captcha",
        "forbidden",
        "minimal_text, no_content_elements",
    ]
    if any(marker in text for marker in blocked_markers):
        return "blocked", "anti_bot_or_access_denied"
    if "err_name_not_resolved" in text:
        return "failed", "dns_resolution_failed"
    if "err_connection_refused" in text:
        return "failed", "connection_refused"
    if "timeout" in text:
        return "failed", "timeout"
    return "failed", "crawl_failed"


def truncate_detail(detail: Optional[str], limit: int = 1600) -> str:
    text = (detail or "").replace("\n", " ").replace("\r", " ").strip()
    if len(text) <= limit:
        return text
    return text[:limit]


def emit_crawl_audit(
    *,
    normalized_key: str,
    url: str,
    report: str,
    reason: str,
    duration_ms: int,
    status_code: Optional[int],
    error_detail: Optional[str],
    redirected_url: Optional[str] = None,
    content_type: Optional[str] = None,
) -> None:
    event = {
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "normalized_key": normalized_key,
        "url": url,
        "report": report,
        "reason": reason,
        "status_code": status_code,
        "duration_ms": duration_ms,
    }
    if error_detail:
        event["error_detail"] = truncate_detail(error_detail)
    if redirected_url:
        event["redirected_url"] = redirected_url
    if content_type:
        event["content_type"] = content_type
    audit_logger.info(json.dumps(event, ensure_ascii=True, separators=(",", ":")))


@app.on_event("startup")
async def log_browser_profile() -> None:
    configure_file_logging()
    cfg = browser_config()
    run_cfg = crawler_run_config()
    ua = to_string_or_none(getattr(cfg, "user_agent", None)) or "unset"
    if len(ua) > 160:
        ua = ua[:160]
    logger.warning(
        "crawl browser profile browser=%s headless=%s viewport=%sx%s locale=%s timezone=%s accept_language=%s ua=%s",
        to_string_or_none(getattr(cfg, "browser_type", None)) or "chromium",
        to_string_or_none(getattr(cfg, "headless", None)) or "true",
        to_string_or_none(getattr(cfg, "viewport_width", None)) or "?",
        to_string_or_none(getattr(cfg, "viewport_height", None)) or "?",
        to_string_or_none(getattr(run_cfg, "locale", None)) or "unset",
        to_string_or_none(getattr(run_cfg, "timezone_id", None)) or "unset",
        to_string_or_none((getattr(cfg, "headers", {}) or {}).get("Accept-Language"))
        or "unset",
        ua,
    )


@app.get("/healthz")
async def health_check():
    return {"status": "ok"}


@app.post("/crawl", response_model=CrawlResponse)
async def crawl_endpoint(request: CrawlRequest):
    logger.warning(
        "crawl request received normalized_key=%s url=%s",
        request.normalized_key,
        request.url,
    )
    started = perf_counter()
    try:
        crawl_result = await run_crawl(request)
        emit_crawl_audit(
            normalized_key=request.normalized_key,
            url=str(request.url),
            report="success",
            reason="ok",
            duration_ms=int((perf_counter() - started) * 1000),
            status_code=crawl_result.status_code,
            error_detail=None,
            redirected_url=(crawl_result.metadata or {}).get("redirected_url"),
            content_type=crawl_result.content_type,
        )
    except HTTPException:
        raise
    except CrawlOutcomeError as exc:
        emit_crawl_audit(
            normalized_key=request.normalized_key,
            url=str(request.url),
            report=exc.report,
            reason=exc.reason,
            duration_ms=exc.duration_ms,
            status_code=exc.status_code,
            error_detail=exc.detail,
        )
        logger.warning(
            "crawl request outcome normalized_key=%s url=%s report=%s reason=%s status_code=%s",
            request.normalized_key,
            request.url,
            exc.report,
            exc.reason,
            exc.status_code,
        )
        raise HTTPException(status_code=502, detail=exc.detail) from exc
    except Exception as exc:  # pragma: no cover - defensive
        detail = str(exc)
        report, reason = classify_report(detail, None)
        emit_crawl_audit(
            normalized_key=request.normalized_key,
            url=str(request.url),
            report=report,
            reason=reason,
            duration_ms=int((perf_counter() - started) * 1000),
            status_code=None,
            error_detail=detail,
        )
        logger.exception(
            "crawl request failed normalized_key=%s url=%s",
            request.normalized_key,
            request.url,
        )
        raise HTTPException(status_code=502, detail=str(exc)) from exc
    return crawl_result


async def run_crawl(request: CrawlRequest) -> CrawlResponse:
    started = perf_counter()
    cfg = browser_config()
    crawl_cfg = crawler_run_config()
    try:
        async with AsyncWebCrawler(config=cfg) as crawler:
            result = await crawler.arun(url=str(request.url), config=crawl_cfg)
    except Exception as exc:  # pragma: no cover - defensive fallback
        detail = f"crawl4ai execution failed: {exc}"
        report, reason = classify_report(detail, None)
        raise CrawlOutcomeError(
            report=report,
            reason=reason,
            detail=detail,
            status_code=None,
            duration_ms=int((perf_counter() - started) * 1000),
        ) from exc

    if not result.success:
        detail = f"crawl4ai returned unsuccessful result: {result.error_message or 'crawl failed'}"
        report, reason = classify_report(detail, result.status_code)
        raise CrawlOutcomeError(
            report=report,
            reason=reason,
            detail=detail,
            status_code=result.status_code,
            duration_ms=int((perf_counter() - started) * 1000),
        )

    markdown_text = extract_markdown_text(result).strip()
    if len(markdown_text) > request.max_text_chars:
        markdown_text = markdown_text[: request.max_text_chars]

    content_type = (
        result.response_headers.get("content-type")
        if result.response_headers
        else "text/html"
    )
    metadata = {
        "cache_status": to_string_or_none(result.cache_status),
        "redirected_url": to_string_or_none(result.redirected_url),
    }

    return CrawlResponse(
        normalized_key=request.normalized_key,
        url=request.url,
        status="ok",
        markdown_text=markdown_text,
        cleaned_text=markdown_text,
        content_type=content_type or "text/html",
        language=to_string_or_none((result.metadata or {}).get("language")),
        title=to_string_or_none((result.metadata or {}).get("title")),
        status_code=result.status_code,
        metadata={k: v for k, v in metadata.items() if v is not None},
    )


def extract_markdown_text(result) -> str:
    if result.markdown is not None:
        raw_markdown = to_string_or_none(getattr(result.markdown, "raw_markdown", None))
        if raw_markdown:
            return raw_markdown
    html_source = result.cleaned_html or result.html or ""
    if not html_source:
        return ""
    without_tags = re.sub(r"(?is)<[^>]+>", " ", html_source)
    normalized = re.sub(r"\s+", " ", without_tags)
    return normalized.strip()


def to_string_or_none(value) -> Optional[str]:
    if value is None:
        return None
    text = str(value).strip()
    return text or None


if __name__ == "__main__":  # pragma: no cover
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=int(os.getenv("PORT", "8085")))

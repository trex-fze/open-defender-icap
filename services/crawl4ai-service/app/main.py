import logging
import os
import re
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


@app.on_event("startup")
async def log_browser_profile() -> None:
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
    try:
        crawl_result = await run_crawl(request)
    except HTTPException:
        raise
    except Exception as exc:  # pragma: no cover - defensive
        logger.exception(
            "crawl request failed normalized_key=%s url=%s",
            request.normalized_key,
            request.url,
        )
        raise HTTPException(status_code=502, detail=str(exc)) from exc
    return crawl_result


async def run_crawl(request: CrawlRequest) -> CrawlResponse:
    cfg = browser_config()
    crawl_cfg = crawler_run_config()
    try:
        async with AsyncWebCrawler(config=cfg) as crawler:
            result = await crawler.arun(url=str(request.url), config=crawl_cfg)
    except Exception as exc:  # pragma: no cover - defensive fallback
        raise HTTPException(
            status_code=502, detail=f"crawl4ai execution failed: {exc}"
        ) from exc

    if not result.success:
        raise HTTPException(
            status_code=502,
            detail=f"crawl4ai returned unsuccessful result: {result.error_message or 'crawl failed'}",
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

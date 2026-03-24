import asyncio
import os
from functools import lru_cache
from typing import Optional

import requests
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


class CrawlRequest(BaseModel):
    url: HttpUrl
    normalized_key: str = Field(..., min_length=1)
    max_html_bytes: int = Field(524_288, ge=4_096, le=2_097_152)
    max_text_chars: int = Field(4_000, ge=512, le=40_000)


class CrawlResponse(BaseModel):
    normalized_key: str
    url: HttpUrl
    status: str
    cleaned_text: str
    raw_html: str
    content_type: str
    language: Optional[str]
    title: Optional[str]
    status_code: Optional[int]
    metadata: dict


@lru_cache(maxsize=1)
def browser_config() -> BrowserConfig:
    return BrowserConfig(
        headless=bool_env("CRAWL4AI_HEADLESS", "true"),
        browser_type=os.getenv("CRAWL4AI_BROWSER", "chromium"),
        user_agent=os.getenv("CRAWL4AI_USER_AGENT"),
        verbose=bool_env("CRAWL4AI_VERBOSE", "false"),
    )


def crawler_run_config() -> CrawlerRunConfig:
    return CrawlerRunConfig(
        cache_mode=CacheMode.BYPASS,
        scan_full_page=True,
        process_iframes=True,
        wait_until="networkidle",
        delay_before_return_html=0.2,
    )


app = FastAPI(title="Crawl4AI Service", version="0.1.0")


@app.get("/healthz")
async def health_check():
    return {"status": "ok"}


@app.post("/crawl", response_model=CrawlResponse)
async def crawl_endpoint(request: CrawlRequest):
    try:
        crawl_result = await run_crawl(request)
    except HTTPException:
        raise
    except Exception as exc:  # pragma: no cover - defensive
        raise HTTPException(status_code=502, detail=str(exc)) from exc
    return crawl_result


async def run_crawl(request: CrawlRequest) -> CrawlResponse:
    cfg = browser_config()
    crawl_cfg = crawler_run_config()
    try:
        async with AsyncWebCrawler(config=cfg) as crawler:
            result = await crawler.arun(url=str(request.url), config=crawl_cfg)
    except Exception as exc:  # pragma: no cover - defensive fallback
        return await fallback_fetch(request, exc)

    if not result.success:
        return await fallback_fetch(
            request, RuntimeError(result.error_message or "crawl failed")
        )

    cleaned_text = extract_text(result) or ""
    cleaned_text = cleaned_text.strip()
    if len(cleaned_text) > request.max_text_chars:
        cleaned_text = cleaned_text[: request.max_text_chars]

    raw_html = result.cleaned_html or result.html or ""
    if len(raw_html.encode("utf-8")) > request.max_html_bytes:
        raw_html = raw_html.encode("utf-8")[: request.max_html_bytes].decode(
            "utf-8", errors="ignore"
        )

    content_type = (
        result.response_headers.get("content-type")
        if result.response_headers
        else "text/html"
    )
    metadata = {
        "dispatch": result.dispatch_result.model_dump()
        if result.dispatch_result
        else None,
        "cache_status": result.cache_status,
        "redirected_url": result.redirected_url,
        "crawl_stats": result.crawl_stats,
    }

    return CrawlResponse(
        normalized_key=request.normalized_key,
        url=request.url,
        status="ok",
        cleaned_text=cleaned_text,
        raw_html=raw_html,
        content_type=content_type or "text/html",
        language=(result.metadata or {}).get("language"),
        title=(result.metadata or {}).get("title"),
        status_code=result.status_code,
        metadata={k: v for k, v in metadata.items() if v is not None},
    )


async def fallback_fetch(
    request: CrawlRequest, original_exc: Exception
) -> CrawlResponse:
    """Best-effort backup using simple HTTP request when browser automation fails."""

    def fetch() -> requests.Response:
        headers = {
            "User-Agent": os.getenv("CRAWL4AI_FALLBACK_UA", "OpenDefenderSmoke/1.0")
        }
        return requests.get(str(request.url), headers=headers, timeout=10)

    try:
        response = await asyncio.to_thread(fetch)
        response.raise_for_status()
    except Exception as exc:  # pragma: no cover - defensive
        raise HTTPException(
            status_code=502,
            detail=f"crawl fallback failed: {exc}; original error: {original_exc}",
        ) from exc

    raw_html = response.text or ""
    cleaned_text = raw_html.strip()
    if len(cleaned_text) > request.max_text_chars:
        cleaned_text = cleaned_text[: request.max_text_chars]

    encoded = raw_html.encode("utf-8")
    if len(encoded) > request.max_html_bytes:
        raw_html = encoded[: request.max_html_bytes].decode("utf-8", errors="ignore")

    metadata = {
        "fallback": "requests",
        "status_code": response.status_code,
        "original_error": str(original_exc),
    }

    return CrawlResponse(
        normalized_key=request.normalized_key,
        url=request.url,
        status="ok",
        cleaned_text=cleaned_text,
        raw_html=raw_html,
        content_type=response.headers.get("content-type", "text/html"),
        language=None,
        title=None,
        status_code=response.status_code,
        metadata=metadata,
    )


def extract_text(result) -> str:
    if result.markdown is not None:
        return result.markdown.raw_markdown
    if result.cleaned_html:
        return result.cleaned_html
    return result.html or ""


if __name__ == "__main__":  # pragma: no cover
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=int(os.getenv("PORT", "8085")))

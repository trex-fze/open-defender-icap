use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{de::DeserializeOwned, Serialize};

#[derive(Debug, Clone, Copy)]
pub struct PageOptions {
    pub page: u32,
    pub page_size: u32,
}

impl PageOptions {
    pub fn new(page: Option<u32>, page_size: Option<u32>) -> Self {
        let page = page.unwrap_or(1).max(1);
        let page_size = page_size.unwrap_or(50).clamp(1, 200);
        Self { page, page_size }
    }

    pub fn offset(&self) -> i64 {
        ((self.page.saturating_sub(1)) as i64) * (self.page_size as i64)
    }
}

#[derive(Debug, Serialize)]
pub struct PageMeta {
    pub page: u32,
    pub page_size: u32,
    pub total: i64,
    pub has_more: bool,
}

#[derive(Debug, Serialize)]
pub struct Paged<T> {
    pub data: Vec<T>,
    pub meta: PageMeta,
}

impl<T> Paged<T> {
    pub fn new(data: Vec<T>, total: i64, opts: PageOptions) -> Self {
        let has_more = (opts.page as i64 * opts.page_size as i64) < total;
        Self {
            data,
            meta: PageMeta {
                page: opts.page,
                page_size: opts.page_size,
                total,
                has_more,
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CursorMeta {
    pub limit: u32,
    pub has_more: bool,
    pub next_cursor: Option<String>,
    pub prev_cursor: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CursorPaged<T> {
    pub data: Vec<T>,
    pub meta: CursorMeta,
}

impl<T> CursorPaged<T> {
    pub fn new(data: Vec<T>, limit: u32, has_more: bool, next_cursor: Option<String>) -> Self {
        Self {
            data,
            meta: CursorMeta {
                limit,
                has_more,
                next_cursor,
                prev_cursor: None,
            },
        }
    }
}

pub fn cursor_limit(limit: Option<u32>) -> u32 {
    limit.unwrap_or(50).clamp(1, 200)
}

pub fn encode_cursor<T: Serialize>(cursor: &T) -> Result<String, serde_json::Error> {
    let bytes = serde_json::to_vec(cursor)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

pub fn decode_cursor<T: DeserializeOwned>(cursor: &str) -> Result<T, String> {
    let decoded = URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| "invalid cursor encoding".to_string())?;
    serde_json::from_slice::<T>(&decoded).map_err(|_| "invalid cursor payload".to_string())
}

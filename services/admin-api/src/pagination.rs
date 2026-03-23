use serde::Serialize;

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

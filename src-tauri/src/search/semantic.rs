use crate::asset::models::{Asset, CoreSearchRequest, Page};
pub fn available() -> bool {
    false
}
pub fn search(
    _db: &rusqlite::Connection,
    _request: &CoreSearchRequest,
) -> Result<Page<Asset>, String> {
    Err("未配置语义检索".into())
}

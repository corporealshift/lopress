use chrono::NaiveDate;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct RenderContext<'a> {
    pub site: &'a SiteCtx,
    pub page: &'a PageCtx,
}

#[derive(Debug, Clone, Serialize)]
pub struct SiteCtx {
    pub title: String,
    pub base_url: String,
    pub nav: Vec<NavItem>,
    pub posts: Vec<PostSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NavItem {
    pub label: String,
    pub href: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostSummary {
    pub title: String,
    pub slug: String,
    pub url: String,
    pub date: Option<NaiveDate>,
    pub tags: Vec<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PageCtx {
    pub kind: PageKind,
    pub title: String,
    pub slug: String,
    pub url: String,
    pub canonical: String,
    pub description: Option<String>,
    pub og_image: Option<String>,
    pub date: Option<NaiveDate>,
    pub tags: Vec<String>,
    pub body_html: String,
    pub posts: Vec<PostSummary>,
    pub tag: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PageKind {
    Index,
    Post,
    Page,
    Tag,
    NotFound,
}

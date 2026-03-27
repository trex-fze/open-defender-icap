use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    env, fmt, fs,
    path::{Path, PathBuf},
    sync::Arc,
};

mod activation;

pub use activation::ActivationState;

pub const DEFAULT_TAXONOMY_PATH: &str = "config/canonical-taxonomy.json";
pub const UNKNOWN_CATEGORY_ID: &str = "unknown-unclassified";
const UNKNOWN_DEFAULT_SUBCATEGORY_ID: &str = "insufficient-evidence";

#[derive(Debug, Clone, Deserialize)]
pub struct CanonicalTaxonomy {
    pub version: String,
    #[serde(default)]
    pub source: Option<String>,
    pub categories: Vec<CanonicalCategory>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CanonicalCategory {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub always_enabled: Option<bool>,
    #[serde(default)]
    pub subcategories: Vec<CanonicalSubcategory>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CanonicalSubcategory {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub always_enabled: Option<bool>,
}

impl CanonicalTaxonomy {
    pub fn load_from_env() -> Result<Self> {
        if let Ok(path) = env::var("OD_CANONICAL_TAXONOMY_PATH") {
            return Self::load(Path::new(&path));
        }
        let default = resolve_default_path()?;
        Self::load(&default)
    }

    pub fn load(path: &Path) -> Result<Self> {
        let body = fs::read_to_string(path).with_context(|| {
            format!(
                "failed to read canonical taxonomy file at {}",
                path.display()
            )
        })?;
        let taxonomy: CanonicalTaxonomy = serde_json::from_str(&body).with_context(|| {
            format!(
                "failed to parse canonical taxonomy JSON at {}",
                path.display()
            )
        })?;
        taxonomy.validate()?;
        Ok(taxonomy)
    }

    fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            !self.version.trim().is_empty(),
            "taxonomy version must be non-empty",
        );
        let mut category_ids = HashSet::new();
        for category in &self.categories {
            anyhow::ensure!(
                category_ids.insert(category.id.as_str()),
                "duplicate category id detected: {}",
                category.id
            );
            anyhow::ensure!(
                !category.subcategories.is_empty(),
                "category {} must contain at least one subcategory",
                category.id
            );
            let mut sub_ids = HashSet::new();
            for sub in &category.subcategories {
                anyhow::ensure!(
                    sub_ids.insert(sub.id.as_str()),
                    "duplicate subcategory id detected within {}: {}",
                    category.id,
                    sub.id
                );
            }
        }
        anyhow::ensure!(
            self.categories.len() == 40,
            "expected 40 canonical categories, found {}",
            self.categories.len()
        );
        anyhow::ensure!(
            self.categories
                .iter()
                .any(|cat| cat.id == UNKNOWN_CATEGORY_ID),
            "missing unknown-unclassified category"
        );
        Ok(())
    }

    pub fn into_arc(self) -> Arc<Self> {
        Arc::new(self)
    }
}

#[derive(Debug)]
pub struct TaxonomyStore {
    taxonomy: Arc<CanonicalTaxonomy>,
    category_by_id: HashMap<String, usize>,
    category_lookup: HashMap<String, usize>,
    subcategory_by_id: HashMap<String, HashMap<String, usize>>,
    subcategory_lookup: HashMap<String, HashMap<String, usize>>,
    unknown_category_index: usize,
    unknown_default_subcategory_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackReason {
    MissingCategory,
    UnknownCategory,
    MissingSubcategory,
    UnknownSubcategory,
}

#[derive(Debug)]
pub struct ValidatedTaxonomyLabels<'a> {
    pub category: &'a CanonicalCategory,
    pub subcategory: &'a CanonicalSubcategory,
    pub fallback_reason: Option<FallbackReason>,
    pub normalized_category: Option<String>,
    pub normalized_subcategory: Option<String>,
}

impl TaxonomyStore {
    pub fn new(taxonomy: Arc<CanonicalTaxonomy>) -> Self {
        let mut category_by_id = HashMap::new();
        let mut category_lookup = HashMap::new();
        let mut subcategory_by_id: HashMap<String, HashMap<String, usize>> = HashMap::new();
        let mut subcategory_lookup: HashMap<String, HashMap<String, usize>> = HashMap::new();
        let mut unknown_category_index = None;
        let mut unknown_default_subcategory_index = None;

        for (index, category) in taxonomy.categories.iter().enumerate() {
            category_by_id.insert(category.id.clone(), index);
            if let Some(normalized) = normalize_label(&category.name) {
                category_lookup.insert(normalized, index);
            }
            if let Some(normalized) = normalize_label(&category.id) {
                category_lookup.entry(normalized).or_insert(index);
            }

            if category.id == UNKNOWN_CATEGORY_ID {
                unknown_category_index = Some(index);
            }

            let mut subs_by_id = HashMap::new();
            let mut subs_lookup = HashMap::new();
            for (sub_index, sub) in category.subcategories.iter().enumerate() {
                subs_by_id.insert(sub.id.clone(), sub_index);
                if let Some(normalized) = normalize_label(&sub.name) {
                    subs_lookup.insert(normalized, sub_index);
                }
                if let Some(normalized) = normalize_label(&sub.id) {
                    subs_lookup.entry(normalized).or_insert(sub_index);
                }
                if category.id == UNKNOWN_CATEGORY_ID && sub.id == UNKNOWN_DEFAULT_SUBCATEGORY_ID {
                    unknown_default_subcategory_index = Some(sub_index);
                }
            }
            subcategory_by_id.insert(category.id.clone(), subs_by_id);
            subcategory_lookup.insert(category.id.clone(), subs_lookup);
        }

        let unknown_category_index = unknown_category_index.expect("missing unknown category");
        let unknown_default_subcategory_index = unknown_default_subcategory_index.unwrap_or(0);

        let mut store = Self {
            taxonomy,
            category_by_id,
            category_lookup,
            subcategory_by_id,
            subcategory_lookup,
            unknown_category_index,
            unknown_default_subcategory_index,
        };

        store.apply_category_aliases();
        store.apply_subcategory_aliases();
        store
    }

    pub fn load_default() -> Result<Self> {
        Ok(Self::new(CanonicalTaxonomy::load_from_env()?.into_arc()))
    }

    pub fn taxonomy(&self) -> Arc<CanonicalTaxonomy> {
        self.taxonomy.clone()
    }

    pub fn validate_labels<'a>(
        &'a self,
        category: &str,
        subcategory: Option<&str>,
    ) -> ValidatedTaxonomyLabels<'a> {
        let trimmed_category = category.trim();
        let normalized_category = normalize_label(trimmed_category);
        let category_resolution =
            self.resolve_category(trimmed_category, normalized_category.as_deref());

        let mut category_index = category_resolution.index;
        let mut fallback_reason = category_resolution.fallback_reason;

        let cleaned_subcategory = subcategory.and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });
        let normalized_subcategory = cleaned_subcategory.and_then(normalize_label);

        let subcategory_index;
        if category_index == self.unknown_category_index {
            subcategory_index = self.unknown_default_subcategory_index;
        } else {
            match self.resolve_subcategory(
                category_index,
                cleaned_subcategory,
                normalized_subcategory.as_deref(),
            ) {
                Some(SubcategoryResolution::Match(idx)) => {
                    subcategory_index = idx;
                }
                Some(SubcategoryResolution::Fallback(reason)) => {
                    category_index = self.unknown_category_index;
                    subcategory_index = self.unknown_default_subcategory_index;
                    if fallback_reason.is_none() {
                        fallback_reason = Some(reason);
                    }
                }
                None => {
                    category_index = self.unknown_category_index;
                    subcategory_index = self.unknown_default_subcategory_index;
                    if fallback_reason.is_none() {
                        fallback_reason = Some(FallbackReason::MissingSubcategory);
                    }
                }
            }
        }

        let category_ref = self.category_by_index(category_index);
        let subcategory_ref = self.subcategory_by_index(category_index, subcategory_index);

        ValidatedTaxonomyLabels {
            category: category_ref,
            subcategory: subcategory_ref,
            fallback_reason,
            normalized_category,
            normalized_subcategory,
        }
    }

    fn category_by_index(&self, index: usize) -> &CanonicalCategory {
        &self.taxonomy.categories[index]
    }

    fn subcategory_by_index(
        &self,
        category_index: usize,
        subcategory_index: usize,
    ) -> &CanonicalSubcategory {
        &self.taxonomy.categories[category_index].subcategories[subcategory_index]
    }

    fn resolve_category(&self, input: &str, normalized: Option<&str>) -> CategoryResolution {
        if let Some(idx) = self.category_by_id.get(input) {
            return CategoryResolution {
                index: *idx,
                fallback_reason: None,
            };
        }

        if let Some(norm) = normalized {
            if let Some(idx) = self.category_lookup.get(norm) {
                return CategoryResolution {
                    index: *idx,
                    fallback_reason: None,
                };
            }
        }

        if input.is_empty() {
            CategoryResolution {
                index: self.unknown_category_index,
                fallback_reason: Some(FallbackReason::MissingCategory),
            }
        } else {
            CategoryResolution {
                index: self.unknown_category_index,
                fallback_reason: Some(FallbackReason::UnknownCategory),
            }
        }
    }

    fn resolve_subcategory(
        &self,
        category_index: usize,
        input: Option<&str>,
        normalized: Option<&str>,
    ) -> Option<SubcategoryResolution> {
        let category = &self.taxonomy.categories[category_index];
        let by_id = self.subcategory_by_id.get(&category.id)?;
        if let Some(value) = input {
            if let Some(idx) = by_id.get(value) {
                return Some(SubcategoryResolution::Match(*idx));
            }
        }

        if let Some(norm) = normalized {
            if let Some(map) = self.subcategory_lookup.get(&category.id) {
                if let Some(idx) = map.get(norm) {
                    return Some(SubcategoryResolution::Match(*idx));
                }
            }
        }

        if input.is_none() {
            Some(SubcategoryResolution::Fallback(
                FallbackReason::MissingSubcategory,
            ))
        } else {
            Some(SubcategoryResolution::Fallback(
                FallbackReason::UnknownSubcategory,
            ))
        }
    }

    fn apply_category_aliases(&mut self) {
        for (alias, category_id) in CATEGORY_ALIAS_PAIRS {
            if let Some(idx) = self.category_by_id.get(*category_id) {
                if let Some(normalized) = normalize_label(alias) {
                    self.category_lookup.entry(normalized).or_insert(*idx);
                }
            }
        }
    }

    fn apply_subcategory_aliases(&mut self) {
        for (category_id, alias, subcategory_id) in SUBCATEGORY_ALIAS_PAIRS {
            let Some(sub_map) = self.subcategory_by_id.get(*category_id) else {
                continue;
            };
            let Some(&sub_idx) = sub_map.get(*subcategory_id) else {
                continue;
            };
            if let Some(normalized) = normalize_label(alias) {
                if let Some(lookup) = self.subcategory_lookup.get_mut(*category_id) {
                    lookup.entry(normalized).or_insert(sub_idx);
                }
            }
        }
    }
}

struct CategoryResolution {
    index: usize,
    fallback_reason: Option<FallbackReason>,
}

enum SubcategoryResolution {
    Match(usize),
    Fallback(FallbackReason),
}

fn normalize_label(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut buf = String::with_capacity(trimmed.len());
    let mut last_was_space = false;
    for ch in trimmed.chars() {
        let lower = ch.to_ascii_lowercase();
        let mapped = match lower {
            '0'..='9' => lower,
            'a'..='z' => lower,
            _ => ' ',
        };
        if mapped == ' ' {
            if !last_was_space {
                buf.push(' ');
                last_was_space = true;
            }
        } else {
            buf.push(mapped);
            last_was_space = false;
        }
    }
    let normalized = buf.trim().to_string();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

const CATEGORY_ALIAS_PAIRS: &[(&str, &str)] = &[
    ("social", "social-media"),
    ("social networking", "social-media"),
    ("shopping", "shopping-e-commerce"),
    ("ecommerce", "shopping-e-commerce"),
    ("malware", "malware-phishing-fraud"),
    ("phishing", "malware-phishing-fraud"),
    ("fraud", "malware-phishing-fraud"),
    ("adult", "adult-sexual-content"),
    ("sexual content", "adult-sexual-content"),
    ("unknown", "unknown-unclassified"),
];

const SUBCATEGORY_ALIAS_PAIRS: &[(&str, &str, &str)] = &[
    ("social-media", "short form video", "short-video-platforms"),
    ("social-media", "social networking", "social-networks"),
    ("social-media", "social network", "social-networks"),
    (
        "social-media",
        "general social networking",
        "social-networks",
    ),
    ("news-media", "general", "general-news"),
    ("shopping-e-commerce", "retail", "general-retail"),
    (
        UNKNOWN_CATEGORY_ID,
        "unknown",
        UNKNOWN_DEFAULT_SUBCATEGORY_ID,
    ),
];

impl FallbackReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            FallbackReason::MissingCategory => "missing_category",
            FallbackReason::UnknownCategory => "unknown_category",
            FallbackReason::MissingSubcategory => "missing_subcategory",
            FallbackReason::UnknownSubcategory => "unknown_subcategory",
        }
    }
}

impl fmt::Display for FallbackReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

fn resolve_default_path() -> Result<PathBuf> {
    let cwd = env::current_dir().context("failed to determine current directory")?;
    for ancestor in cwd.ancestors() {
        let candidate = ancestor.join(DEFAULT_TAXONOMY_PATH);
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(anyhow!(
        "canonical taxonomy not found; set OD_CANONICAL_TAXONOMY_PATH"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canonical_path() -> PathBuf {
        super::resolve_default_path().expect("canonical taxonomy path")
    }

    #[test]
    fn canonical_taxonomy_file_is_valid() {
        let taxonomy =
            CanonicalTaxonomy::load(&canonical_path()).expect("canonical taxonomy should parse");
        assert_eq!(taxonomy.categories.len(), 40);
        assert!(taxonomy
            .categories
            .iter()
            .any(|cat| cat.id == UNKNOWN_CATEGORY_ID));
    }

    #[test]
    fn resolves_aliases() {
        let taxonomy = CanonicalTaxonomy::load(&canonical_path())
            .expect("canonical taxonomy should parse")
            .into_arc();
        let store = TaxonomyStore::new(taxonomy);
        let result = store.validate_labels("Social", Some("Short form video"));
        assert_eq!(result.category.id, "social-media");
        assert_eq!(result.subcategory.id, "short-video-platforms");
        assert!(result.fallback_reason.is_none());
    }

    #[test]
    fn resolves_social_networking_alias_variants() {
        let taxonomy = CanonicalTaxonomy::load(&canonical_path())
            .expect("canonical taxonomy should parse")
            .into_arc();
        let store = TaxonomyStore::new(taxonomy);
        for input in [
            "Social Networking",
            "social network",
            "General Social Networking",
        ] {
            let result = store.validate_labels("social-media", Some(input));
            assert_eq!(result.category.id, "social-media");
            assert_eq!(result.subcategory.id, "social-networks");
            assert!(
                result.fallback_reason.is_none(),
                "alias should resolve: {input}"
            );
        }
    }

    #[test]
    fn falls_back_to_unknown() {
        let taxonomy = CanonicalTaxonomy::load(&canonical_path())
            .expect("canonical taxonomy should parse")
            .into_arc();
        let store = TaxonomyStore::new(taxonomy);
        let result = store.validate_labels("NotReal", Some("Mystery"));
        assert_eq!(result.category.id, UNKNOWN_CATEGORY_ID);
        assert_eq!(result.subcategory.id, UNKNOWN_DEFAULT_SUBCATEGORY_ID);
        assert_eq!(
            result.fallback_reason,
            Some(FallbackReason::UnknownCategory)
        );
    }
}

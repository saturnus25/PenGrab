use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fs,
    path::Path,
};
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};
use unicode_width::UnicodeWidthChar;

pub type AppResult<T> = Result<T, Box<dyn Error>>;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    En,
    Es,
}

#[derive(Deserialize)]
pub struct Localized {
    pub en: String,
    #[serde(default)]
    pub es: String,
}

impl Localized {
    pub fn get(&self, language: Language) -> &str {
        match language {
            Language::Es if !self.es.is_empty() => &self.es,
            _ => &self.en,
        }
    }
}

#[derive(Deserialize)]
struct CategoryInput {
    id: String,
    name: Localized,
    #[serde(default)]
    aliases: Vec<String>,
    symbol: Option<char>,
    icon: Option<CategoryIcon>,
    #[serde(default)]
    children: Vec<CategoryInput>,
}

#[derive(Clone, Copy, Deserialize)]
pub struct CategoryIcon {
    pub glyph: char,
    pub accent: Option<IconAccent>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IconAccent {
    Teal,
    Blue,
    Gold,
}

pub struct Category {
    pub id: String,
    pub name: Localized,
    pub aliases: Vec<String>,
    pub symbol: Option<char>,
    pub icon: Option<CategoryIcon>,
    pub parent: Option<usize>,
    pub depth: usize,
    pub children: Vec<usize>,
    pub tools: Vec<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub category: String,
    pub description: Localized,
    pub winget_id: Option<String>,
    pub kali: Option<String>,
    pub publisher_download: Option<String>,
    pub publisher_project: Option<String>,
}

impl Tool {
    pub fn publisher_url(&self) -> Option<&str> {
        self.publisher_download
            .as_deref()
            .or(self.publisher_project.as_deref())
    }

    pub fn winget_install_command(&self) -> Option<String> {
        let id = self.winget_id.as_deref()?;
        if id.starts_with(|ch: char| ch.is_ascii_alphanumeric())
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        {
            Some(format!("winget install --id {id} --exact --source winget"))
        } else {
            None
        }
    }
}

pub struct Catalog {
    pub categories: Vec<Category>,
    pub tools: Vec<Tool>,
    pub all: Vec<usize>,
    roots: Vec<usize>,
    tool_categories: Vec<usize>,
    category_lookup: HashMap<String, usize>,
}

impl Catalog {
    pub fn load(directory: &Path) -> AppResult<Self> {
        let read = |name: &str| {
            let path = directory.join(name);
            fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))
        };

        let categories = read("categories.json")?;
        let tools = read("tools.json")?;
        Self::from_json(&categories, &tools)
    }

    pub fn from_json(categories: &str, tools: &str) -> AppResult<Self> {
        let categories: Vec<CategoryInput> =
            serde_json::from_str(categories.trim_start_matches('\u{feff}'))
                .map_err(|error| format!("categories.json: {error}"))?;
        let tools: Vec<Tool> = serde_json::from_str(tools.trim_start_matches('\u{feff}'))
            .map_err(|error| format!("tools.json: {error}"))?;

        let mut catalog = Self {
            categories: Vec::new(),
            all: (0..tools.len()).collect(),
            roots: Vec::new(),
            tool_categories: Vec::with_capacity(tools.len()),
            category_lookup: HashMap::new(),
            tools,
        };
        for entry in categories {
            let index = catalog.add_category(entry, None)?;
            catalog.roots.push(index);
        }

        let mut seen = HashSet::new();
        for (index, tool) in catalog.tools.iter().enumerate() {
            let fields = [&tool.id, &tool.name, &tool.description.en];
            if fields
                .iter()
                .any(|value| value.trim().is_empty() || value.chars().any(char::is_control))
            {
                return Err(format!(
                    "tools.json: invalid id, name or English description for entry {}",
                    index + 1
                )
                .into());
            }
            if !seen.insert(tool.id.as_str()) {
                return Err(format!("tools.json: duplicate tool id '{}'", tool.id).into());
            }
            for url in tool
                .publisher_download
                .iter()
                .chain(&tool.publisher_project)
            {
                if url.strip_prefix("https://").is_some_and(|rest| {
                    rest.starts_with(|ch: char| ch.is_ascii_alphanumeric())
                        && rest.bytes().all(|byte| {
                            byte.is_ascii_alphanumeric() || b"-._~:/?#[]@!$&()*+;=%".contains(&byte)
                        })
                }) {
                    continue;
                }
                return Err(format!(
                    "tools.json: '{}' needs a valid HTTPS publisher URL",
                    tool.id
                )
                .into());
            }
            let category = catalog.category_index(&tool.category).ok_or_else(|| {
                format!(
                    "tools.json: '{}' references unknown category '{}'",
                    tool.id, tool.category
                )
            })?;
            catalog.tool_categories.push(category);
            let mut cursor = Some(category);
            while let Some(node) = cursor {
                catalog.categories[node].tools.push(index);
                cursor = catalog.categories[node].parent;
            }
        }

        Ok(catalog)
    }

    fn add_category(&mut self, entry: CategoryInput, parent: Option<usize>) -> AppResult<usize> {
        if entry.id.trim().is_empty()
            || entry.id.chars().any(char::is_control)
            || entry.name.en.trim().is_empty()
            || entry.name.en.chars().any(char::is_control)
        {
            return Err("categories.json: each category needs a valid id and English name".into());
        }
        if entry
            .symbol
            .is_some_and(|symbol| !symbol.is_ascii_graphic())
        {
            return Err(format!(
                "categories.json: '{}' needs a printable ASCII symbol",
                entry.id
            )
            .into());
        }
        if entry
            .icon
            .is_some_and(|icon| icon.glyph.is_whitespace() || icon.glyph.width() != Some(1))
        {
            return Err(format!(
                "categories.json: '{}' needs an icon that occupies one terminal cell",
                entry.id
            )
            .into());
        }
        let index = self.categories.len();
        if self
            .category_lookup
            .insert(entry.id.clone(), index)
            .is_some()
        {
            return Err(format!("categories.json: duplicate category '{}'", entry.id).into());
        }
        let depth = parent.map_or(0, |node| self.categories[node].depth + 1);
        self.categories.push(Category {
            id: entry.id,
            name: entry.name,
            aliases: entry.aliases,
            symbol: entry.symbol,
            icon: entry.icon,
            parent,
            depth,
            children: Vec::new(),
            tools: Vec::new(),
        });
        for child in entry.children {
            let child = self.add_category(child, Some(index))?;
            self.categories[index].children.push(child);
        }
        Ok(index)
    }

    pub fn category_index(&self, id: &str) -> Option<usize> {
        self.category_lookup.get(id).copied()
    }

    pub fn branch(&self, category: Option<usize>) -> &[usize] {
        category.map_or(&self.all, |index| &self.categories[index].tools)
    }

    pub fn tool_category(&self, index: usize) -> usize {
        self.tool_categories[index]
    }

    pub fn category_line(&self, tool: usize, language: Language) -> String {
        let mut parts = Vec::new();
        let mut node = Some(self.tool_categories[tool]);
        while let Some(index) = node {
            let category = &self.categories[index];
            parts.push(category.name.get(language));
            node = category.parent;
        }
        parts.reverse();
        parts.join(" / ")
    }

    pub fn collapsed_branches(&self) -> HashSet<usize> {
        self.categories
            .iter()
            .enumerate()
            .filter_map(|(index, category)| (!category.children.is_empty()).then_some(index))
            .collect()
    }

    pub fn visible_categories(&self, closed: &HashSet<usize>) -> Vec<Option<usize>> {
        let mut rows = vec![None];
        let mut next: Vec<usize> = self.roots.iter().rev().copied().collect();
        while let Some(index) = next.pop() {
            rows.push(Some(index));
            if !closed.contains(&index) {
                next.extend(self.categories[index].children.iter().rev().copied());
            }
        }
        rows
    }

    pub fn contains_words(&self, index: usize, words: &[&str], language: Language) -> bool {
        if words.is_empty() {
            return true;
        }
        let tool = &self.tools[index];
        let fields = [
            &tool.name,
            &tool.id,
            &tool.category,
            tool.winget_id.as_deref().unwrap_or_default(),
            tool.description.get(language),
            &self.category_line(index, language),
        ];
        let haystack = search_text(&fields.join(" "));
        let aliases: Vec<_> = tool
            .aliases
            .iter()
            .map(|alias| search_text(alias))
            .collect();
        words
            .iter()
            .all(|word| haystack.contains(word) || aliases.iter().any(|alias| alias.contains(word)))
    }
}

pub fn search_text(text: &str) -> String {
    text.nfd()
        .filter(|letter| !is_combining_mark(*letter))
        .flat_map(char::to_lowercase)
        .collect()
}

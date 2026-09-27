use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SidePanelPageFormat {
    #[default]
    Markdown,
    Pdf,
}

impl SidePanelPageFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Pdf => "pdf",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SidePanelPageSource {
    #[default]
    Managed,
    LinkedFile,
    Ephemeral,
}

impl SidePanelPageSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Managed => "managed",
            Self::LinkedFile => "linked_file",
            Self::Ephemeral => "ephemeral",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistedSidePanelState {
    #[serde(default)]
    pub focus_revision: u64,
    #[serde(default)]
    pub focused_page_id: Option<String>,
    #[serde(default)]
    pub pages: Vec<PersistedSidePanelPage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedSidePanelPage {
    pub id: String,
    pub title: String,
    pub file_path: String,
    #[serde(default)]
    pub format: SidePanelPageFormat,
    #[serde(default)]
    pub source: SidePanelPageSource,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SidePanelPage {
    pub id: String,
    pub title: String,
    pub file_path: String,
    #[serde(default)]
    pub format: SidePanelPageFormat,
    #[serde(default)]
    pub source: SidePanelPageSource,
    #[serde(default)]
    pub content: String,
    /// Base64 PDF bytes. Content remains a human-readable Markdown fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pdf_data: Option<String>,
    #[serde(default)]
    pub updated_at_ms: u64,
}

impl SidePanelPage {
    /// An ephemeral markdown page: in-memory content, not backed by a file.
    ///
    /// Every mirror page (todos, split view, observe, catch-up) has this exact
    /// shape, so the shape lives with the type instead of four times over.
    pub fn ephemeral_markdown(
        id: impl Into<String>,
        title: impl Into<String>,
        file_path: impl Into<String>,
        content: impl Into<String>,
        updated_at_ms: u64,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            file_path: file_path.into(),
            format: SidePanelPageFormat::Markdown,
            source: SidePanelPageSource::Ephemeral,
            content: content.into(),
            pdf_data: None,
            updated_at_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SidePanelSnapshot {
    #[serde(default)]
    pub focus_revision: u64,
    #[serde(default)]
    pub focused_page_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pages: Vec<SidePanelPage>,
}

impl SidePanelSnapshot {
    pub fn has_pages(&self) -> bool {
        !self.pages.is_empty()
    }

    pub fn focused_page(&self) -> Option<&SidePanelPage> {
        let focused_id = self.focused_page_id.as_deref()?;
        self.pages.iter().find(|page| page.id == focused_id)
    }
}

pub fn snapshot_is_empty(snapshot: &SidePanelSnapshot) -> bool {
    !snapshot.has_pages()
}

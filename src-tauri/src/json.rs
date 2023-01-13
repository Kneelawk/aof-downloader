use serde::Deserialize;
use serde::Serialize;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestJson {
    pub manifest_type: String,
    pub version: String,
    pub files: Vec<FileJson>,
    pub manifest_version: i64,
    pub name: String,
    pub overrides: String,
    pub author: String,
    pub minecraft: MinecraftJson,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileJson {
    #[serde(rename = "projectID")]
    pub project_id: i64,
    #[serde(rename = "fileID")]
    pub file_id: i64,
    pub download_url: String,
    pub required: bool,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinecraftJson {
    pub version: String,
    pub mod_loaders: Vec<ModLoaderJson>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModLoaderJson {
    pub id: String,
    pub primary: bool,
}

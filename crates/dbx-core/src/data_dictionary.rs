use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::agent_service::AgentProgressEvent;
use crate::connection::AppState;
use crate::database_export::{is_export_cancelled, ExportProgress, ExportStatus};
use crate::db::{ColumnInfo, IndexInfo};
use crate::models::connection::DatabaseType;
use crate::plugins::{PluginManifest, SUPPORTED_PLUGIN_PROTOCOL_VERSION};
use crate::update::{fetch_latest_release, is_newer_version, JdbcPluginLatest};

pub const DATA_DICTIONARY_PLUGIN_ID: &str = "data-dictionary";
const PLUGIN_DOWNLOAD_URL: &str =
    "https://github.com/t8y2/dbx/releases/latest/download/dbx-data-dictionary-plugin-latest.zip";
const PLUGIN_R2_PATH: &str = "releases/latest/dbx-data-dictionary-plugin-latest.zip";
/// Rendering a large schema through the sidecar can exceed the default 30s
/// plugin timeout; document generation is CPU-bound on the plugin side.
const RENDER_TIMEOUT: Duration = Duration::from_secs(600);

pub const DATA_DICTIONARY_FORMATS: &[&str] = &["word", "excel", "html", "markdown"];

#[derive(Debug, Clone, Serialize)]
pub struct DataDictionaryPluginStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub protocol_version: Option<u32>,
    pub compatible: bool,
    pub latest_version: Option<String>,
    pub latest_protocol_version: Option<u32>,
    pub update_available: bool,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataDictionaryExportRequest {
    pub connection_id: String,
    pub database: String,
    #[serde(default)]
    pub schema: String,
    /// One of `word` / `excel` / `html` / `markdown`.
    pub format: String,
    /// Final destination chosen by the user, including the file name.
    pub file_path: String,
    /// Table names to export; empty means every table in the schema.
    #[serde(default)]
    pub tables: Vec<String>,
    #[serde(default = "default_true")]
    pub include_indexes: bool,
    pub export_id: String,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataDictionaryExportResult {
    pub file_path: String,
    pub table_count: usize,
}

// ---- Plugin install / status ----

pub async fn get_data_dictionary_plugin_status(plugins_root: &Path) -> Result<DataDictionaryPluginStatus, String> {
    let plugin_dir = plugins_root.join(DATA_DICTIONARY_PLUGIN_ID);
    let manifest = read_manifest(&plugin_dir)?;
    let latest = latest_data_dictionary_plugin().await;
    Ok(build_plugin_status(&manifest, latest.as_ref(), &plugin_dir))
}

pub async fn install_data_dictionary_plugin_with_progress(
    plugins_root: &Path,
    progress: impl Fn(AgentProgressEvent),
) -> Result<DataDictionaryPluginStatus, String> {
    let bytes = download_plugin_zip_with_progress(&progress).await?;
    let plugin_dir = plugins_root.join(DATA_DICTIONARY_PLUGIN_ID);
    let status_dir = plugin_dir.clone();
    progress(AgentProgressEvent::transfer("data-dictionary-plugin-extract", 0, 0));
    tokio::task::spawn_blocking(move || install_plugin_zip(&bytes, &plugin_dir))
        .await
        .map_err(|err| err.to_string())??;
    let manifest = read_manifest(&status_dir)?;
    let latest = latest_data_dictionary_plugin().await;
    progress(AgentProgressEvent::step("done"));
    Ok(build_plugin_status(&manifest, latest.as_ref(), &status_dir))
}

pub async fn install_data_dictionary_plugin_from_file(
    plugins_root: &Path,
    file_path: &str,
) -> Result<DataDictionaryPluginStatus, String> {
    let plugin_dir = plugins_root.join(DATA_DICTIONARY_PLUGIN_ID);
    let status_dir = plugin_dir.clone();
    let file_path = file_path.to_string();
    tokio::task::spawn_blocking(move || {
        let bytes = std::fs::read(file_path).map_err(|e| format!("Failed to read file: {e}"))?;
        install_plugin_zip(&bytes, &plugin_dir)
    })
    .await
    .map_err(|err| err.to_string())??;
    let manifest = read_manifest(&status_dir)?;
    let latest = latest_data_dictionary_plugin().await;
    Ok(build_plugin_status(&manifest, latest.as_ref(), &status_dir))
}

pub fn uninstall_data_dictionary_plugin(plugins_root: &Path) -> Result<DataDictionaryPluginStatus, String> {
    let plugin_dir = plugins_root.join(DATA_DICTIONARY_PLUGIN_ID);
    if plugin_dir.exists() {
        std::fs::remove_dir_all(&plugin_dir).map_err(|err| err.to_string())?;
    }
    Ok(build_plugin_status(&None, None, &plugin_dir))
}

fn read_manifest(plugin_dir: &Path) -> Result<Option<PluginManifest>, String> {
    match std::fs::read_to_string(plugin_dir.join("manifest.json")) {
        Ok(raw) => Ok(Some(serde_json::from_str::<PluginManifest>(&raw).map_err(|err| err.to_string())?)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.to_string()),
    }
}

fn build_plugin_status(
    manifest: &Option<PluginManifest>,
    latest: Option<&JdbcPluginLatest>,
    plugin_dir: &Path,
) -> DataDictionaryPluginStatus {
    let version = manifest.as_ref().and_then(|m| (!m.version.is_empty()).then_some(m.version.clone()));
    let protocol_version = manifest.as_ref().map(|m| m.protocol_version);
    let compatible = match manifest.as_ref() {
        Some(m) => m.protocol_version == SUPPORTED_PLUGIN_PROTOCOL_VERSION,
        None => true,
    };
    let update_available = match (version.as_deref(), latest) {
        (Some(current), Some(latest)) if manifest.is_some() => is_newer_version(&latest.version, current),
        (None, Some(_)) if manifest.is_some() => true,
        _ => false,
    };
    DataDictionaryPluginStatus {
        installed: manifest.is_some(),
        version,
        protocol_version,
        compatible,
        latest_version: latest.map(|plugin| plugin.version.clone()),
        latest_protocol_version: latest.map(|plugin| plugin.protocol_version),
        update_available,
        path: plugin_dir.to_string_lossy().to_string(),
    }
}

async fn latest_data_dictionary_plugin() -> Option<JdbcPluginLatest> {
    fetch_latest_release("zh-CN", crate::DownloadSource::Official)
        .await
        .ok()
        .and_then(|release| release.data_dictionary_plugin)
}

async fn download_plugin_zip_with_progress(progress: &impl Fn(AgentProgressEvent)) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|err| err.to_string())?;

    let mut resp =
        crate::race_download(&client, PLUGIN_DOWNLOAD_URL, PLUGIN_R2_PATH, "dbx-data-dictionary-plugin-installer")
            .await
            .map_err(|err| format!("Failed to download data dictionary plugin: {err}"))?;

    let total = resp.content_length().unwrap_or(0);
    progress(AgentProgressEvent::transfer("data-dictionary-plugin", 0, total));
    let mut downloaded = 0;
    let mut bytes = Vec::with_capacity(total.try_into().unwrap_or(0));
    while let Some(chunk) = resp.chunk().await.map_err(|err| err.to_string())? {
        downloaded += chunk.len() as u64;
        bytes.extend_from_slice(&chunk);
        progress(AgentProgressEvent::transfer("data-dictionary-plugin", downloaded, total));
    }
    if total == 0 {
        progress(AgentProgressEvent::transfer("data-dictionary-plugin", downloaded, downloaded));
    }
    Ok(bytes)
}

fn install_plugin_zip(bytes: &[u8], plugin_dir: &Path) -> Result<(), String> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|err| err.to_string())?;
    let temp_dir = plugin_dir.with_extension("tmp");
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir).map_err(|err| err.to_string())?;
    }
    std::fs::create_dir_all(&temp_dir).map_err(|err| err.to_string())?;

    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|err| err.to_string())?;
        if file.is_dir() {
            continue;
        }
        let Some(enclosed) = file.enclosed_name().map(|path| path.to_path_buf()) else {
            continue;
        };
        let relative = crate::jdbc::strip_zip_root(&enclosed);
        if relative.as_os_str().is_empty() {
            continue;
        }
        let output = temp_dir.join(relative);
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let mut target = std::fs::File::create(&output).map_err(|err| err.to_string())?;
        std::io::copy(&mut file, &mut target).map_err(|err| err.to_string())?;
    }

    let manifest_path = temp_dir.join("manifest.json");
    if !manifest_path.exists() {
        let _ = std::fs::remove_dir_all(&temp_dir);
        return Err("Downloaded data dictionary plugin package is missing manifest.json".to_string());
    }
    let manifest = std::fs::read_to_string(&manifest_path)
        .map_err(|err| format!("Failed to read downloaded data dictionary plugin manifest: {err}"))?;
    let manifest: PluginManifest = serde_json::from_str(&manifest)
        .map_err(|err| format!("Failed to parse downloaded data dictionary plugin manifest: {err}"))?;
    if manifest.id != DATA_DICTIONARY_PLUGIN_ID {
        let _ = std::fs::remove_dir_all(&temp_dir);
        return Err(format!("Downloaded plugin has unexpected id '{}'", manifest.id));
    }
    if manifest.protocol_version != SUPPORTED_PLUGIN_PROTOCOL_VERSION {
        let _ = std::fs::remove_dir_all(&temp_dir);
        return Err(format!(
            "Downloaded data dictionary plugin uses protocol version {}, but this DBX build supports protocol version {}",
            manifest.protocol_version, SUPPORTED_PLUGIN_PROTOCOL_VERSION
        ));
    }

    if plugin_dir.exists() {
        std::fs::remove_dir_all(plugin_dir).map_err(|err| err.to_string())?;
    }
    std::fs::rename(&temp_dir, plugin_dir).map_err(|err| err.to_string())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let executable = plugin_dir.join("bin").join("dbx-data-dictionary-plugin");
        if executable.exists() {
            let mut permissions = std::fs::metadata(&executable).map_err(|err| err.to_string())?.permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(executable, permissions).map_err(|err| err.to_string())?;
        }
    }

    Ok(())
}

// ---- Export ----

struct CollectedTable {
    name: String,
    comment: Option<String>,
    columns: Vec<ColumnInfo>,
    indexes: Vec<IndexInfo>,
}

pub async fn export_data_dictionary_core(
    state: &AppState,
    request: &DataDictionaryExportRequest,
    on_progress: impl Fn(ExportProgress) + Sync,
) -> Result<DataDictionaryExportResult, String> {
    if !DATA_DICTIONARY_FORMATS.contains(&request.format.as_str()) {
        return Err(format!("Unsupported data dictionary format: {}", request.format));
    }
    if state.plugins.find_driver(DATA_DICTIONARY_PLUGIN_ID)?.is_none() {
        return Err("The data dictionary plugin is not installed".to_string());
    }

    let db_type = state
        .configs
        .read()
        .await
        .get(&request.connection_id)
        .map(|c| c.db_type)
        .ok_or_else(|| format!("Connection config not found: {}", request.connection_id))?;

    let all_tables = crate::schema::list_tables_core(
        state,
        &request.connection_id,
        &request.database,
        &request.schema,
        None,
        None,
        None,
        None,
        None,
    )
    .await?;

    let selected: Vec<String> = if request.tables.is_empty() {
        all_tables
            .iter()
            .filter(|table| table.table_type.eq_ignore_ascii_case("TABLE"))
            .map(|table| table.name.clone())
            .collect()
    } else {
        // Preserve catalog order while honoring the user's selection.
        all_tables
            .iter()
            .filter(|table| request.tables.iter().any(|name| name == &table.name))
            .map(|table| table.name.clone())
            .collect()
    };
    if selected.is_empty() {
        return Err("No tables to export".to_string());
    }

    let comment_by_table: std::collections::HashMap<&str, Option<String>> =
        all_tables.iter().map(|table| (table.name.as_str(), table.comment.clone())).collect();

    let total = selected.len();
    let mut collected = Vec::with_capacity(total);
    for (index, table) in selected.iter().enumerate() {
        if is_export_cancelled(&request.export_id).await {
            emit(&on_progress, request, table, index, total, ExportStatus::Cancelled, None);
            return Err("Export cancelled".to_string());
        }
        emit(&on_progress, request, table, index, total, ExportStatus::Running, None);

        let columns =
            crate::schema::get_columns_core(state, &request.connection_id, &request.database, &request.schema, table)
                .await?;
        let indexes = if request.include_indexes {
            crate::schema::list_indexes_core(state, &request.connection_id, &request.database, &request.schema, table)
                .await
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        collected.push(CollectedTable {
            name: table.clone(),
            comment: comment_by_table.get(table.as_str()).cloned().flatten(),
            columns,
            indexes,
        });
    }

    if is_export_cancelled(&request.export_id).await {
        emit(&on_progress, request, "", total, total, ExportStatus::Cancelled, None);
        return Err("Export cancelled".to_string());
    }
    emit(&on_progress, request, "", total, total, ExportStatus::Writing, None);

    let staging_dir = std::env::temp_dir().join("dbx-data-dictionary").join(&request.export_id);
    std::fs::create_dir_all(&staging_dir).map_err(|err| format!("Failed to create staging directory: {err}"))?;

    let payload = build_dictionary_payload(
        &request.database,
        data_dictionary_dialect(&db_type),
        &request.format,
        &staging_dir.to_string_lossy(),
        request.include_indexes,
        &collected,
    );

    let render = async {
        let env = state.external_driver_runtime_env(DATA_DICTIONARY_PLUGIN_ID)?;
        let rendered: serde_json::Value = state
            .plugins
            .invoke_driver_with_env_and_timeout(
                DATA_DICTIONARY_PLUGIN_ID,
                "renderDataDictionary",
                payload,
                env,
                Some(RENDER_TIMEOUT),
            )
            .await?;
        let rendered_path = rendered
            .get("filePath")
            .and_then(serde_json::Value::as_str)
            .ok_or("Data dictionary plugin did not return a file path")?
            .to_string();
        move_rendered_file(Path::new(&rendered_path), Path::new(&request.file_path))
    };
    let result = render.await;
    let _ = std::fs::remove_dir_all(&staging_dir);
    result?;

    emit(&on_progress, request, "", total, total, ExportStatus::Done, None);
    Ok(DataDictionaryExportResult { file_path: request.file_path.clone(), table_count: total })
}

fn emit(
    on_progress: &(impl Fn(ExportProgress) + Sync),
    request: &DataDictionaryExportRequest,
    current_object: &str,
    object_index: usize,
    total_objects: usize,
    status: ExportStatus,
    error: Option<String>,
) {
    let preparing = matches!(status, ExportStatus::Running);
    on_progress(ExportProgress {
        export_id: request.export_id.clone(),
        current_object: current_object.to_string(),
        object_index,
        total_objects,
        rows_exported: 0,
        total_rows: None,
        status,
        error,
        preparing,
    });
}

fn move_rendered_file(source: &Path, destination: &Path) -> Result<(), String> {
    if let Some(parent) = destination.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|err| format!("Failed to create output directory: {err}"))?;
        }
    }
    if std::fs::rename(source, destination).is_ok() {
        return Ok(());
    }
    // The staging directory may live on another filesystem; fall back to copy.
    std::fs::copy(source, destination).map_err(|err| format!("Failed to write output file: {err}"))?;
    std::fs::remove_file(source).map_err(|err| err.to_string())?;
    Ok(())
}

/// Maps DBX's database types onto database-export's eight document layouts.
/// The layout only controls which columns appear in the rendered tables, so
/// families map to their closest dialect and everything else falls back to
/// the generic MySQL layout (name/type/nullable/primary/auto-increment/default/comment).
pub fn data_dictionary_dialect(db_type: &DatabaseType) -> &'static str {
    match db_type {
        DatabaseType::Postgres
        | DatabaseType::Redshift
        | DatabaseType::Kingbase
        | DatabaseType::Highgo
        | DatabaseType::Vastbase
        | DatabaseType::Gaussdb
        | DatabaseType::OpenGauss
        | DatabaseType::Uxdb
        | DatabaseType::Questdb => "POSTGRESQL",
        DatabaseType::Oracle | DatabaseType::OceanbaseOracle | DatabaseType::Yashandb => "ORACLE",
        DatabaseType::SqlServer => "SQLSERVER",
        DatabaseType::ClickHouse => "CLICKHOUSE",
        DatabaseType::Sqlite
        | DatabaseType::Rqlite
        | DatabaseType::Turso
        | DatabaseType::CloudflareD1
        | DatabaseType::DuckDb => "SQLITE",
        DatabaseType::Dameng => "DM",
        DatabaseType::Db2 => "DB2",
        _ => "MYSQL",
    }
}

fn build_dictionary_payload(
    database_name: &str,
    dialect: &str,
    format: &str,
    output_dir: &str,
    include_indexes: bool,
    tables: &[CollectedTable],
) -> serde_json::Value {
    serde_json::json!({
        "format": format,
        "dialect": dialect,
        "databaseName": database_name,
        "outputDir": output_dir,
        "searchIndex": include_indexes,
        "tables": tables
            .iter()
            .map(|table| {
                serde_json::json!({
                    "name": table.name,
                    "comment": table.comment.clone().unwrap_or_default(),
                    "columns": table.columns.iter().map(map_column).collect::<Vec<_>>(),
                    "indexes": table
                        .indexes
                        .iter()
                        .enumerate()
                        .map(|(position, index)| map_index(index, position + 1))
                        .collect::<Vec<_>>(),
                })
            })
            .collect::<Vec<_>>(),
    })
}

/// Field names follow database-export's column bean properties; the plugin
/// fills every matching public field of the dialect's dynamic bean.
fn map_column(column: &ColumnInfo) -> serde_json::Value {
    let data_length = column.character_maximum_length.or(column.numeric_precision);
    serde_json::json!({
        "columnName": column.name,
        "dataType": column.data_type,
        "nullAble": column.is_nullable,
        "primary": column.is_primary_key,
        "autoIncrement": is_auto_increment(column),
        "defaultVal": column.column_default,
        "comments": column.comment.clone().unwrap_or_default(),
        "dataLength": data_length.map(|value| value.to_string()),
        "dataScale": column.numeric_scale.map(|value| value.to_string()),
    })
}

fn map_index(index: &IndexInfo, sequence: usize) -> serde_json::Value {
    serde_json::json!({
        "name": index.name,
        "fields": index.columns.join(", "),
        "type": index.index_type,
        "indexType": index.index_type,
        "unique": index.is_unique,
        "seqIndex": sequence,
        "indexId": sequence,
        "comments": index.comment.clone().unwrap_or_default(),
    })
}

fn is_auto_increment(column: &ColumnInfo) -> bool {
    let extra = column.extra.as_deref().unwrap_or_default().to_ascii_lowercase();
    if extra.contains("auto_increment") || extra.contains("identity") {
        return true;
    }
    column
        .column_default
        .as_deref()
        .map(|value| value.trim_start().to_ascii_lowercase().starts_with("nextval("))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(name: &str) -> ColumnInfo {
        ColumnInfo { name: name.to_string(), data_type: "varchar".to_string(), ..Default::default() }
    }

    #[test]
    fn dialect_mapping_covers_dedicated_layouts_and_falls_back() {
        assert_eq!(data_dictionary_dialect(&DatabaseType::Mysql), "MYSQL");
        assert_eq!(data_dictionary_dialect(&DatabaseType::Doris), "MYSQL");
        assert_eq!(data_dictionary_dialect(&DatabaseType::Postgres), "POSTGRESQL");
        assert_eq!(data_dictionary_dialect(&DatabaseType::Kingbase), "POSTGRESQL");
        assert_eq!(data_dictionary_dialect(&DatabaseType::Oracle), "ORACLE");
        assert_eq!(data_dictionary_dialect(&DatabaseType::SqlServer), "SQLSERVER");
        assert_eq!(data_dictionary_dialect(&DatabaseType::ClickHouse), "CLICKHOUSE");
        assert_eq!(data_dictionary_dialect(&DatabaseType::DuckDb), "SQLITE");
        assert_eq!(data_dictionary_dialect(&DatabaseType::Dameng), "DM");
        assert_eq!(data_dictionary_dialect(&DatabaseType::Db2), "DB2");
        assert_eq!(data_dictionary_dialect(&DatabaseType::MongoDb), "MYSQL");
    }

    #[test]
    fn payload_maps_columns_to_database_export_fields() {
        let mut id = column("id");
        id.data_type = "bigint".to_string();
        id.is_nullable = false;
        id.is_primary_key = true;
        id.extra = Some("auto_increment".to_string());
        id.comment = Some("主键".to_string());
        id.numeric_precision = Some(20);
        id.numeric_scale = Some(0);

        let table = CollectedTable {
            name: "users".to_string(),
            comment: Some("用户表".to_string()),
            columns: vec![id],
            indexes: vec![IndexInfo {
                name: "pk_users".to_string(),
                columns: vec!["id".to_string(), "tenant_id".to_string()],
                is_unique: true,
                is_primary: true,
                filter: None,
                index_type: Some("BTREE".to_string()),
                included_columns: None,
                comment: None,
            }],
        };

        let payload = build_dictionary_payload("demo", "MYSQL", "word", "/tmp/out", true, &[table]);
        assert_eq!(payload["format"], "word");
        assert_eq!(payload["dialect"], "MYSQL");
        assert_eq!(payload["databaseName"], "demo");
        assert_eq!(payload["searchIndex"], true);

        let table = &payload["tables"][0];
        assert_eq!(table["name"], "users");
        assert_eq!(table["comment"], "用户表");
        let column = &table["columns"][0];
        assert_eq!(column["columnName"], "id");
        assert_eq!(column["dataType"], "bigint");
        assert_eq!(column["nullAble"], false);
        assert_eq!(column["primary"], true);
        assert_eq!(column["autoIncrement"], true);
        assert_eq!(column["comments"], "主键");
        assert_eq!(column["dataLength"], "20");
        assert_eq!(column["dataScale"], "0");
        let index = &table["indexes"][0];
        assert_eq!(index["fields"], "id, tenant_id");
        assert_eq!(index["seqIndex"], 1);
        assert_eq!(index["unique"], true);
    }

    #[test]
    fn auto_increment_detects_mysql_extra_and_postgres_sequences() {
        let mut mysql = column("id");
        mysql.extra = Some("AUTO_INCREMENT".to_string());
        assert!(is_auto_increment(&mysql));

        let mut identity = column("id");
        identity.extra = Some("identity always".to_string());
        assert!(is_auto_increment(&identity));

        let mut serial = column("id");
        serial.column_default = Some("nextval('users_id_seq'::regclass)".to_string());
        assert!(is_auto_increment(&serial));

        assert!(!is_auto_increment(&column("plain")));
    }

    #[test]
    fn install_rejects_zip_with_wrong_plugin_id() {
        let mut buffer = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            writer.start_file("manifest.json", options).unwrap();
            use std::io::Write;
            writer.write_all(br#"{"id":"jdbc","name":"x","version":"1","protocol_version":1,"drivers":[]}"#).unwrap();
            writer.finish().unwrap();
        }
        let target = std::env::temp_dir().join(format!("dbx-dd-test-{}", uuid::Uuid::new_v4()));
        let err = install_plugin_zip(buffer.get_ref(), &target).unwrap_err();
        assert!(err.contains("unexpected id"), "{err}");
        assert!(!target.exists());
    }

    #[test]
    fn install_accepts_valid_zip_and_replaces_existing() {
        let mut buffer = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            writer.start_file("dbx-data-dictionary-plugin-0.1.0/manifest.json", options).unwrap();
            use std::io::Write;
            writer
                .write_all(
                    br#"{"id":"data-dictionary","name":"x","version":"0.1.0","protocol_version":1,"executable":"bin/dbx-data-dictionary-plugin","drivers":[{"id":"data-dictionary","label":"Data Dictionary","kind":"exporter"}]}"#,
                )
                .unwrap();
            writer.start_file("dbx-data-dictionary-plugin-0.1.0/lib/dbx-data-dictionary-plugin.jar", options).unwrap();
            writer.write_all(b"jar-bytes").unwrap();
            writer.finish().unwrap();
        }
        let target = std::env::temp_dir().join(format!("dbx-dd-test-{}", uuid::Uuid::new_v4()));
        install_plugin_zip(buffer.get_ref(), &target).unwrap();
        assert!(target.join("manifest.json").exists());
        assert!(target.join("lib").join("dbx-data-dictionary-plugin.jar").exists());

        // Re-install over the existing directory succeeds (atomic replace).
        install_plugin_zip(buffer.get_ref(), &target).unwrap();
        assert!(target.join("manifest.json").exists());
        let _ = std::fs::remove_dir_all(&target);
    }
}

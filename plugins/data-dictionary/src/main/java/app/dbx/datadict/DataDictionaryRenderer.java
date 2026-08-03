package app.dbx.datadict;

import com.fasterxml.jackson.databind.JsonNode;
import io.github.pomzwj.dbexport.core.constant.DataBaseConfigConstant;
import io.github.pomzwj.dbexport.core.domain.DbBaseInfo;
import io.github.pomzwj.dbexport.core.domain.DbColumnInfo;
import io.github.pomzwj.dbexport.core.domain.DbExportConfig;
import io.github.pomzwj.dbexport.core.domain.DbIndexInfo;
import io.github.pomzwj.dbexport.core.domain.DbTable;
import io.github.pomzwj.dbexport.core.filegeneration.FileGenerationFactory;
import io.github.pomzwj.dbexport.core.filegeneration.FileGenerationService;
import io.github.pomzwj.dbexport.core.type.DataBaseType;
import io.github.pomzwj.dbexport.core.type.ExportFileType;
import io.github.pomzwj.dbexport.core.utils.ClassUtils;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

/**
 * Renders a data dictionary document from metadata supplied by DBX.
 *
 * DBX collects tables, columns, and indexes through its own drivers and sends
 * them as JSON. This adapter maps that JSON onto database-export's document
 * model and drives its file generation layer directly, skipping the JDBC
 * metadata layer entirely ({@code makeFile} receives a {@code null} DataSource,
 * which is safe because {@code dbBaseInfo} is pre-populated).
 */
final class DataDictionaryRenderer {
    private static final FileGenerationFactory FILE_GENERATION_FACTORY = new FileGenerationFactory();
    /** Bundled templates required by the word/html renderers. The PDF font is intentionally not shipped. */
    private static final String[] TEMPLATE_RESOURCES = {
        DataBaseConfigConstant.IMPORT_TEMPLATE,
        DataBaseConfigConstant.SUB_MODEL_TEMPLATE,
        DataBaseConfigConstant.HTML_TEMPLATE,
    };
    private static volatile boolean templatesReady;

    private DataDictionaryRenderer() {
    }

    static String render(JsonNode params) throws Exception {
        String format = requireText(params, "format");
        String databaseName = requireText(params, "databaseName");
        String outputDir = requireText(params, "outputDir");
        JsonNode tablesNode = params.path("tables");
        if (!tablesNode.isArray() || tablesNode.isEmpty()) {
            throw new IllegalArgumentException("No tables to export");
        }

        ExportFileType fileType = resolveFileType(format);
        DataBaseType dialect = resolveDialect(params.path("dialect").asText(""));
        boolean searchIndex = params.path("searchIndex").asBoolean(true);

        ensureTemplates();

        DbExportConfig config = new DbExportConfig()
            .setExportFileTypeEnum(fileType)
            .setGenerationFileTempDir(outputDir)
            .setSearchIndex(searchIndex);
        Class<? extends DbColumnInfo> columnClazz = ClassUtils
            .createDbColumBean(dialect.getColumnInfoClazz(), null)
            .load(DbColumnInfo.class.getClassLoader())
            .getLoaded();
        Class<? extends DbIndexInfo> indexClazz = ClassUtils
            .createDbIndexBean(dialect.getIndexInfoClazz(), null)
            .load(DbIndexInfo.class.getClassLoader())
            .getLoaded();
        config.setDbColumnInfoDynamicClazz(columnClazz);
        config.setDbIndexInfoDynamicClazz(indexClazz);
        applyBaseInfo(config, databaseName, dialect);

        List<DbTable> tables = new ArrayList<>();
        for (JsonNode tableNode : tablesNode) {
            tables.add(buildTable(tableNode, columnClazz, indexClazz));
        }

        FileGenerationService generation = FILE_GENERATION_FACTORY.getFileGenerationBean(fileType);
        if (generation == null) {
            throw new IllegalArgumentException("Unsupported export format: " + format);
        }
        File file = generation.makeFile(null, config, tables);
        return file.getAbsolutePath();
    }

    private static ExportFileType resolveFileType(String format) {
        return switch (format.toLowerCase(Locale.ROOT)) {
            case "word" -> ExportFileType.WORD;
            case "excel" -> ExportFileType.EXCEL;
            case "html" -> ExportFileType.HTML;
            case "markdown" -> ExportFileType.MARKDOWN;
            case "pdf" -> throw new IllegalArgumentException(
                "PDF export is not available in this plugin build");
            default -> throw new IllegalArgumentException("Unsupported export format: " + format);
        };
    }

    private static DataBaseType resolveDialect(String dialect) {
        if (dialect != null && !dialect.isBlank()) {
            DataBaseType matched = DataBaseType.matchType(dialect.trim());
            if (matched != null) {
                return matched;
            }
        }
        // Generic column layout (name/type/nullable/primary/auto-increment/default/comment)
        // for databases without a dedicated database-export dialect.
        return DataBaseType.MYSQL;
    }

    /**
     * {@link DbExportConfig#getDbBaseInfo()} has no public setter; it is normally
     * populated by {@code checkAndInitConfig(DataSource)}, which requires a live
     * JDBC connection. Setting the field directly keeps credentials out of this
     * process. Covered by unit tests so an upstream rename fails loudly.
     */
    private static void applyBaseInfo(DbExportConfig config, String databaseName, DataBaseType dialect)
        throws ReflectiveOperationException {
        DbBaseInfo baseInfo = new DbBaseInfo();
        baseInfo.setDbName(databaseName);
        baseInfo.setDbKindEnum(dialect);
        Field field = DbExportConfig.class.getDeclaredField("dbBaseInfo");
        field.setAccessible(true);
        field.set(config, baseInfo);
    }

    private static DbTable buildTable(
        JsonNode tableNode,
        Class<? extends DbColumnInfo> columnClazz,
        Class<? extends DbIndexInfo> indexClazz
    ) throws ReflectiveOperationException {
        DbTable table = new DbTable();
        table.setTableName(tableNode.path("name").asText(""));
        table.setTableComments(tableNode.path("comment").asText(""));

        List<DbColumnInfo> columns = new ArrayList<>();
        for (JsonNode columnNode : tableNode.path("columns")) {
            columns.add(populateFields(columnClazz.getDeclaredConstructor().newInstance(), columnNode));
        }
        table.setTabsColumn(columns);

        List<DbIndexInfo> indexes = new ArrayList<>();
        for (JsonNode indexNode : tableNode.path("indexes")) {
            indexes.add(populateFields(indexClazz.getDeclaredConstructor().newInstance(), indexNode));
        }
        table.setTabsIndex(indexes);
        return table;
    }

    /**
     * Fills every public field of the dynamic bean from the JSON object of the
     * same key. The renderers later read these fields reflectively, so driving
     * the mapping from the bean class keeps both sides aligned for all dialects.
     */
    private static <T> T populateFields(T bean, JsonNode source) throws ReflectiveOperationException {
        for (Field field : bean.getClass().getFields()) {
            JsonNode value = source.get(field.getName());
            if (value == null || value.isNull()) {
                continue;
            }
            Class<?> type = field.getType();
            if (type == Boolean.class) {
                field.set(bean, value.asBoolean());
            } else if (type == Integer.class) {
                field.set(bean, value.asInt());
            } else {
                field.set(bean, value.asText());
            }
        }
        return bean;
    }

    /**
     * database-export's renderers load word/html templates from
     * {@code ./.databaseExport}. Its own initializer also extracts the PDF font,
     * which this build excludes, so the required templates are extracted here.
     * The plugin process is started with its working directory set to the
     * plugin's install directory, which is writable.
     */
    private static void ensureTemplates() throws IOException {
        if (templatesReady) {
            return;
        }
        synchronized (DataDictionaryRenderer.class) {
            if (templatesReady) {
                return;
            }
            for (String resource : TEMPLATE_RESOURCES) {
                Path target = Path.of(DataBaseConfigConstant.SYSTEM_FILE_DIR, resource);
                try (InputStream in = DataDictionaryRenderer.class.getClassLoader().getResourceAsStream(resource)) {
                    if (in == null) {
                        throw new IOException("Missing bundled template resource: " + resource);
                    }
                    Files.createDirectories(target.getParent());
                    Files.copy(in, target, StandardCopyOption.REPLACE_EXISTING);
                }
            }
            templatesReady = true;
        }
    }

    private static String requireText(JsonNode params, String key) {
        String value = params.path(key).asText("");
        if (value.isBlank()) {
            throw new IllegalArgumentException("Missing required parameter: " + key);
        }
        return value;
    }
}

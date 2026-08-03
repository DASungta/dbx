package app.dbx.datadict;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import org.apache.poi.xssf.usermodel.XSSFSheet;
import org.apache.poi.xssf.usermodel.XSSFWorkbook;
import org.apache.poi.xwpf.usermodel.XWPFDocument;
import org.apache.poi.xwpf.usermodel.XWPFTable;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.FileInputStream;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class DbxDataDictionaryPluginTest {
    private static final ObjectMapper MAPPER = new ObjectMapper();

    @TempDir
    Path outputDir;

    @Test
    void pingReturnsVersion() throws Exception {
        JsonNode response = request("""
            {"jsonrpc":"2.0","id":7,"driver":"data-dictionary","method":"ping","params":{}}
            """);
        assertEquals(7, response.path("id").asInt());
        assertTrue(response.path("result").path("ok").asBoolean());
        assertEquals(DbxDataDictionaryPlugin.VERSION, response.path("result").path("version").asText());
    }

    @Test
    void unknownMethodReturnsError() throws Exception {
        JsonNode response = request("""
            {"jsonrpc":"2.0","id":2,"method":"bogus","params":{}}
            """);
        assertTrue(response.path("error").path("message").asText().contains("bogus"));
    }

    @Test
    void malformedLineReturnsErrorInsteadOfCrashing() {
        JsonNode response = DbxDataDictionaryPlugin.handleLine("not json at all");
        assertFalse(response.path("error").isMissingNode());
    }

    @Test
    void rendersMarkdownWithChineseComments() throws Exception {
        JsonNode response = renderRequest("markdown", "MYSQL");
        String filePath = assertRenderedFile(response, ".md");
        String content = Files.readString(Path.of(filePath));
        assertTrue(content.contains("users"), "markdown should mention table name");
        assertTrue(content.contains("用户表"), "markdown should keep the Chinese table comment");
        assertTrue(content.contains("主键编号"), "markdown should keep the Chinese column comment");
        assertTrue(content.contains("idx_users_email"), "markdown should include the index");
    }

    @Test
    void rendersHtml() throws Exception {
        JsonNode response = renderRequest("html", "MYSQL");
        String filePath = assertRenderedFile(response, ".html");
        String content = Files.readString(Path.of(filePath));
        assertTrue(content.contains("users"));
        assertTrue(content.contains("用户表"));
    }

    @Test
    void rendersExcelWorkbook() throws Exception {
        JsonNode response = renderRequest("excel", "MYSQL");
        String filePath = assertRenderedFile(response, ".xlsx");
        try (InputStream in = new FileInputStream(filePath); XSSFWorkbook workbook = new XSSFWorkbook(in)) {
            assertTrue(workbook.getNumberOfSheets() > 0);
            XSSFSheet sheet = workbook.getSheetAt(0);
            assertNotNull(sheet);
        }
    }

    @Test
    void rendersWordDocumentContainingTables() throws Exception {
        JsonNode response = renderRequest("word", "MYSQL");
        String filePath = assertRenderedFile(response, ".docx");
        try (InputStream in = new FileInputStream(filePath); XWPFDocument document = new XWPFDocument(in)) {
            List<String> tableTexts = new ArrayList<>();
            for (XWPFTable table : document.getTables()) {
                tableTexts.add(table.getText());
            }
            String allTables = String.join("\n", tableTexts);
            assertTrue(allTables.contains("id"), "word tables should include the column name");
            assertTrue(allTables.contains("主键编号"), "word tables should keep the Chinese column comment");
        }
    }

    @Test
    void unknownDialectFallsBackToGenericLayout() throws Exception {
        JsonNode response = renderRequest("markdown", "QUESTDB");
        String filePath = assertRenderedFile(response, ".md");
        assertTrue(Files.readString(Path.of(filePath)).contains("users"));
    }

    @Test
    void sqlServerDialectUsesItsOwnColumnLayout() throws Exception {
        JsonNode response = renderRequest("markdown", "SQLSERVER");
        String filePath = assertRenderedFile(response, ".md");
        assertTrue(Files.readString(Path.of(filePath)).contains("users"));
    }

    @Test
    void pdfIsRejected() throws Exception {
        JsonNode response = renderRequest("pdf", "MYSQL");
        assertTrue(response.path("error").path("message").asText().contains("PDF"));
    }

    @Test
    void emptyTablesIsRejected() throws Exception {
        JsonNode response = request("""
            {"id":3,"method":"renderDataDictionary","params":{
              "format":"markdown","dialect":"MYSQL","databaseName":"demo",
              "outputDir":"%s","tables":[]}}
            """.formatted(escaped(outputDir)));
        assertTrue(response.path("error").path("message").asText().contains("No tables"));
    }

    @Test
    void missingFormatIsRejected() throws Exception {
        JsonNode response = request("""
            {"id":4,"method":"renderDataDictionary","params":{
              "dialect":"MYSQL","databaseName":"demo",
              "outputDir":"%s","tables":[{"name":"t","columns":[]}]}}
            """.formatted(escaped(outputDir)));
        assertTrue(response.path("error").path("message").asText().contains("format"));
    }

    private JsonNode renderRequest(String format, String dialect) throws Exception {
        return request("""
            {"jsonrpc":"2.0","id":1,"driver":"data-dictionary","method":"renderDataDictionary","params":{
              "format":"%s",
              "dialect":"%s",
              "databaseName":"demo_db",
              "outputDir":"%s",
              "searchIndex":true,
              "tables":[
                {
                  "name":"users",
                  "comment":"用户表",
                  "columns":[
                    {"columnName":"id","dataType":"bigint(20)","nullAble":false,"primary":true,
                     "autoIncrement":true,"defaultVal":null,"comments":"主键编号","dataLength":"20","dataScale":null},
                    {"columnName":"email","dataType":"varchar(255)","nullAble":true,"primary":false,
                     "autoIncrement":false,"defaultVal":"''","comments":"邮箱","dataLength":"255","dataScale":null}
                  ],
                  "indexes":[
                    {"name":"idx_users_email","fields":"email","type":"BTREE","unique":true,"seqIndex":1,"comments":""}
                  ]
                },
                {
                  "name":"orders",
                  "comment":"",
                  "columns":[
                    {"columnName":"order_no","dataType":"varchar(64)","nullAble":false,"primary":true,
                     "autoIncrement":false,"defaultVal":null,"comments":""}
                  ],
                  "indexes":[]
                }
              ]}}
            """.formatted(format, dialect, escaped(outputDir)));
    }

    private String assertRenderedFile(JsonNode response, String expectedSuffix) throws Exception {
        assertTrue(response.path("error").isMissingNode(),
            "render should succeed but failed: " + response.path("error").path("message").asText());
        String filePath = response.path("result").path("filePath").asText();
        assertTrue(filePath.endsWith(expectedSuffix), "unexpected file name: " + filePath);
        Path file = Path.of(filePath);
        assertTrue(Files.isRegularFile(file), "rendered file should exist: " + filePath);
        assertTrue(Files.size(file) > 0, "rendered file should not be empty");
        return filePath;
    }

    private JsonNode request(String line) throws Exception {
        return MAPPER.readTree(DbxDataDictionaryPlugin.handleLine(line.strip()).toString());
    }

    private static String escaped(Path path) {
        return path.toString().replace("\\", "\\\\");
    }
}

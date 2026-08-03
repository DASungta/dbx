package app.dbx.datadict;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;

import java.io.BufferedReader;
import java.io.BufferedWriter;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;

/**
 * DBX data dictionary plugin.
 *
 * Speaks the DBX plugin protocol: newline-delimited JSON-RPC over stdio.
 * The only substantive method is {@code renderDataDictionary}, which receives
 * schema metadata collected by DBX and renders it into a document via the
 * database-export rendering engine. The plugin never opens database
 * connections; credentials stay inside the DBX process.
 */
public final class DbxDataDictionaryPlugin {
    static final ObjectMapper MAPPER = new ObjectMapper();
    static final String VERSION = "0.1.0";

    private DbxDataDictionaryPlugin() {
    }

    public static void main(String[] args) throws Exception {
        try (
            BufferedReader reader = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
            BufferedWriter writer = new BufferedWriter(new OutputStreamWriter(System.out, StandardCharsets.UTF_8))
        ) {
            String line;
            while ((line = reader.readLine()) != null) {
                if (line.isBlank()) {
                    continue;
                }
                ObjectNode response = handleLine(line);
                writer.write(MAPPER.writeValueAsString(response));
                writer.newLine();
                writer.flush();
                if (response.path("_dbx_close").asBoolean(false)) {
                    break;
                }
            }
        }
    }

    static ObjectNode handleLine(String line) {
        ObjectNode response = MAPPER.createObjectNode();
        try {
            JsonNode request = MAPPER.readTree(line);
            JsonNode id = request.path("id");
            response.set("id", id.isMissingNode() ? MAPPER.getNodeFactory().numberNode(1) : id);
            String method = request.path("method").asText("");
            JsonNode params = request.path("params");
            switch (method) {
                case "ping" -> {
                    ObjectNode result = MAPPER.createObjectNode();
                    result.put("ok", true);
                    result.put("version", VERSION);
                    response.set("result", result);
                }
                case "renderDataDictionary" -> {
                    String filePath = DataDictionaryRenderer.render(params);
                    ObjectNode result = MAPPER.createObjectNode();
                    result.put("filePath", filePath);
                    response.set("result", result);
                }
                case "close" -> {
                    ObjectNode result = MAPPER.createObjectNode();
                    result.put("ok", true);
                    response.set("result", result);
                    response.put("_dbx_close", true);
                }
                default -> {
                    ObjectNode errorNode = MAPPER.createObjectNode();
                    errorNode.put("message", "Unknown method: " + method);
                    response.set("error", errorNode);
                }
            }
        } catch (Throwable error) {
            if (!response.has("id")) {
                response.put("id", 1);
            }
            ObjectNode errorNode = MAPPER.createObjectNode();
            errorNode.put("message", throwableMessage(error));
            response.set("error", errorNode);
        }
        return response;
    }

    private static String throwableMessage(Throwable error) {
        List<Throwable> causes = new ArrayList<>();
        Throwable cause = error;
        while (cause != null && !causes.contains(cause)) {
            causes.add(cause);
            cause = cause.getCause();
        }
        for (int i = causes.size() - 1; i >= 0; i--) {
            String message = causes.get(i).getMessage();
            if (message != null && !message.isBlank()) {
                return message.trim();
            }
        }
        return error.toString();
    }
}

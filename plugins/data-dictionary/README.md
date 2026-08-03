# DBX Data Dictionary Plugin

Optional sidecar plugin that renders database schema metadata into data
dictionary documents (Word `.docx`, Excel `.xlsx`, HTML, Markdown). It is not
bundled with the main DBX app and is downloaded on demand, like JDBC drivers.

Rendering is powered by [database-export](https://github.com/PomZWJ/database-export)
(`io.github.pomzwj:database-export-core`, MIT licensed), pulled from Maven
Central at build time. DBX collects tables, columns, and indexes through its
own drivers and sends them to this plugin as JSON over the DBX plugin protocol
(newline-delimited JSON-RPC on stdio); the plugin never receives database
credentials and never opens connections.

PDF output is intentionally disabled: the upstream PDF pipeline depends on
iText (AGPL) and bundles a simsun font, both excluded from this build.

## Build

```sh
mvn -q -DskipTests package
mkdir -p lib
cp target/dbx-data-dictionary-plugin-*-all.jar lib/dbx-data-dictionary-plugin.jar
```

## Package for release

```sh
./package.sh
```

The package version follows `pom.xml` and `manifest.json`. The script writes
both `dbx-data-dictionary-plugin-<version>.zip` and
`dbx-data-dictionary-plugin-latest.zip`.

## Install for local DBX

Copy this folder to the DBX app data plugin directory:

```text
<DBX app data>/plugins/data-dictionary
```

The folder must contain:

```text
manifest.json
bin/dbx-data-dictionary-plugin
lib/dbx-data-dictionary-plugin.jar
```

DBX does not bundle Java. The launcher resolves a JVM from `DBX_JAVA_BIN`
(set by DBX when the managed Java runtime is installed), `JAVA_HOME`, Homebrew
OpenJDK, or `PATH`.

## Protocol

- `ping` → `{ok, version}`
- `renderDataDictionary` → params `{format, dialect, databaseName, outputDir,
  searchIndex, tables: [{name, comment, columns: [...], indexes: [...]}]}`,
  result `{filePath}`. `format` is one of `word|excel|html|markdown`;
  `dialect` is one of database-export's `DataBaseType` names and controls the
  column layout, falling back to the generic MySQL layout when unknown.
- `close` → exits the process.

On first render the plugin extracts the bundled word/html templates into
`.databaseExport/` under its working directory (DBX starts plugins with the
working directory set to the plugin install directory).

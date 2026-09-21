# KI-Entwicklung

Monocurl-Szenen sind reine Textdateien und LLMs können sehr hilfreich sein, um Standard-/mühsame Layout-Aufgaben zu vermeiden.

![An AI agent editing a Monocurl scene while the desktop app previews the result](/img/lessons/ai-mcp-workflow.png)

## Aktueller Status

Monocurl verfügt über einen MCP-Server (Model Context Protocol), der es kompatiblen KI-Assistenten ermöglicht, die Dokumentation, die Stdlib-Referenz, CLI-Notizen und Beispielszenen von Monocurl direkt als strukturierte Ressourcen zu lesen. Dies ist die empfohlene Methode, um einem KI-Assistenten Monocurl-Kontext zu geben.


Der Server wird auf npm als `@enigmurl/monocurl-mcp` veröffentlicht. Es handelt sich lediglich um einen Dokumentationsserver. Es bearbeitet keine Szenen, führt kein Monocurl aus, rendert keine Bilder und validiert keinen Code.

Für einen MCP-Client, der die stdio-Serverkonfiguration akzeptiert, fügen Sie Folgendes hinzu:

```json
{
  "mcpServers": {
    "monocurl": {
      "command": "npx",
      "args": ["-y", "@enigmurl/monocurl-mcp"]
    }
  }
}
```

Für Claude Code lautet der entsprechende Befehl:

```sh
claude mcp add --transport stdio monocurl -- npx -y @enigmurl/monocurl-mcp
```

Fügen Sie für Codex Folgendes zu `~/.codex/config.toml` hinzu:

```toml
[mcp_servers.monocurl]
command = "npx"
args = ["-y", "@enigmurl/monocurl-mcp"]
```

Bitten Sie den Assistenten nach der Installation, die Monocurl MCP-Ressourcen zu lesen, bevor Sie eine Szene schreiben.

## Beispiel-Eingabeaufforderung

Sie können so etwas in einen KI-Codierungsassistenten einfügen:

```text
You are helping me write a Monocurl scene. Monocurl is a programming language and desktop app for creating mathematical animations with live preview, video export, and slideshow-style presentation.

Install and use the Monocurl MCP context server before writing code. If your MCP client supports stdio servers, add a server named monocurl with:

command: npx
args: ["-y", "@enigmurl/monocurl-mcp"]

Then read the Monocurl MCP resources, especially the language overview, stdlib docs, CLI guide, and example scenes.

When writing and testing Monocurl code, keep these points in mind:

1. If the `monocurl` binary is not on PATH, look for the application binary manually. On macOS, check `/Applications`, `~/Applications`, or the app bundle contents. On Windows, check `Program Files` for `Monocurl.exe`. The binary can run scenes and inspect print output.
2. Be mindful when sizing text. Text sizes smaller than `0.5` are typically illegible. Prefer larger text that is easy to read during presentations.
3. In most cases, do not explicitly specify the variables to animate. The default behavior animates all dirty leaders, so the variables list can usually be left as `[]`.
4. Your sandbox may not have access to a GPU device, so CLI image or video rendering may fail. Use print statements and the transcript command heavily to check positioning, values, and the general structure of the scene.
```

## Headless-Modus

Monocurl überwacht Dateiänderungen und aktualisiert die Zeitleiste und das Ansichtsfenster dynamisch als Reaktion auf alle externen Bearbeitungen, einschließlich solcher von KI-Agenten. Folglich können Sie Ihren bevorzugten Texteditor oder einen KI-Agenten verwenden, um die Szenenquelldatei zu schreiben, dann zurück zum Monocurl-Editor wechseln, um die Ergebnisse anzuzeigen und durch die Zeitleiste zu scrollen. Wenn Sie anstelle des Standardeditors lieber den linken Bereich für ein Terminal verwenden möchten, aktivieren Sie das Headless-Menü im Menü „Datei“.

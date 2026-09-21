# Desarrollo de IA

Las escenas Monocurl son archivos de texto sin formato y los LLM pueden ser muy útiles para evitar tareas de diseño repetitivas o tediosas.

![An AI agent editing a Monocurl scene while the desktop app previews the result](/img/lessons/ai-mcp-workflow.png)

## Estado actual

Monocurl tiene un servidor MCP (Model Context Protocol) que permite a los asistentes de IA compatibles leer directamente la documentación de Monocurl, la referencia stdlib, las notas CLI y las escenas de ejemplo como recursos estructurados. Esta es la forma recomendada de darle contexto Monocurl a un asistente de IA.


El servidor está publicado en npm como `@enigmurl/monocurl-mcp`. Es únicamente un servidor de documentación. No edita escenas, ejecuta Monocurl, renderiza imágenes ni valida código.

Para un cliente MCP que acepte la configuración del servidor stdio, agregue:

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

Para Claude Code, el comando equivalente es:

```sh
claude mcp add --transport stdio monocurl -- npx -y @enigmurl/monocurl-mcp
```

Para Codex, agregue esto a `~/.codex/config.toml`:

```toml
[mcp_servers.monocurl]
command = "npx"
args = ["-y", "@enigmurl/monocurl-mcp"]
```

Después de instalarlo, pídale al asistente que lea los recursos de Monocurl MCP antes de escribir una escena.

## Mensaje de ejemplo

Puedes pegar algo como esto en un asistente de codificación de IA:

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

## Modo sin cabeza

Monocurl observará los cambios en los archivos y actualizará dinámicamente la línea de tiempo y la ventana gráfica en respuesta a cualquier edición externa, incluidas las de agentes de IA. En consecuencia, puede utilizar su editor de texto preferido o un agente de IA para escribir el archivo fuente de la escena, luego volver al editor Monocurl para ver los resultados y desplazarse por la línea de tiempo. Si prefiere utilizar el espacio de la izquierda para una terminal en lugar del editor predeterminado, habilite el menú sin cabeza en el menú Archivo.

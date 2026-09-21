# Descripción general

Esta lección explica el editor, la estructura de una escena y cómo pasar de escribir código a exportar o presentar su trabajo.

## El editor

![The Monocurl editor](/img/home/monocurl-editor.png)

El editor tiene tres superficies de trabajo:

- **Editor de código fuente** (izquierda): donde escribe archivos de escena `.mcs`
- **Ventana** (arriba a la derecha): muestra el fotograma renderizado en la posición actual de la línea de tiempo.
- **Línea de tiempo** (abajo a la derecha): recorre las diapositivas y los `play` pasos individuales dentro de cada diapositiva.

Cuando edita código, Monocurl vuelve a evaluar y la vista previa de la ventana gráfica se actualiza según la posición actual en la línea de tiempo.

## Estructura de escena

Cada escena tiene tres partes: importaciones, una sección de inicio y diapositivas. Omitimos las importaciones en la mayoría de los ejemplos por motivos de brevedad.

```mcl 
import std.util
import std.math
import std.color
import std.mesh
import std.anim
import std.scene

# --- init section ---
# runs first; sets up helpers and the initial visible state

mesh dot = center{ORIGIN} fill{soft{} CYAN} stroke{CYAN, 2} Circle(0.4)

slide "intro"
    # slide containing some animations
    mesh title = center{0.8u} Text("Hello", 0.7)
    play Write(0.9)

slide
    dot = center{1.2r} fill{soft{} ORANGE} stroke{ORANGE, 2} Circle(0.5)
    play Lerp(1.2)
```

El código antes del primer `slide` es **init**. Aquí es donde viven las importaciones, junto con las constantes, las funciones auxiliares y el estado visible inicial. Después de la primera palabra clave `slide`, el código pertenece a esa diapositiva y puede contener animaciones `play`.

## Una primera escena

Aquí hay una pequeña escena completa. El patrón es: configurar ciertos ayudantes en init, luego en cada diapositiva mutar el estado de la escena de manera continua a través de animaciones de reproducción.

```mcl video
slide "intro"
    mesh circle = 
      center{pos: 1.4l} 
      color{col: BLUE}
      Circle(0.4)
    mesh label = 
      center{pos:1.4l + 0.75d} 
      Text(text: "hello", 0.65)
    # introduce the newly created meshes in an animated fashion
    play [Write(0.9, [&label]), Fade(0.9, [&circle])]

slide "transform"
    circle.pos = 1.4r
    circle.col = ORANGE
    label.pos = 1.4r + 0.75d
    label.text = "world"
    # transform both meshes into new state
    play Trans(1.2)
```

## Navegación en la línea de tiempo

Para un control detallado, puede hacer clic para buscar. Pero, en general, usa el teclado para moverte por tu escena:

- `,` / `.` — diapositiva anterior/siguiente
- `<` / `>` — saltar al inicio/final de la escena
- `;` / `'` — pequeño paso hacia atrás / hacia adelante


Un hábito útil durante la creación es borrar la línea de tiempo después de agregar una nueva diapositiva para verificar que cada paso `play` haga lo que pretendía.

## Presentar y exportar

El mismo archivo fuente `.mcs` se puede utilizar de tres maneras:

- **Exportación de vídeo** — Menú Archivo → Exportar vídeo. Representa la escena completa como `.mp4`.
- **Exportación de imagen** — Menú Archivo → Exportar imagen. Representa un solo cuadro como `.png`.
- **Modo de presentación** — `Cmd/Ctrl-P`. Convierte las diapositivas en puntos de control de navegación, deteniéndose en cada límite `slide`.

## Flujo de trabajo interactivo

!vid[](/video/interactive-development.mp4)


En el modo de presentación, `Cmd/Ctrl-T` abre el **panel de parámetros** donde puedes editar parte del estado de la escena con controles deslizantes. Esto es más avanzado/nicho pero puede ser poderoso.

En el modo de vista previa y presentación, puede arrastrar el cursor para mover la cámara. Si presiona Mayús y arrastra el cursor, puede desplazar la cámara. Son especialmente útiles para crear escenas en 3D.

## Monorizo ​​en la web

[Ensayos de Monocurl](https://www.monocurl.com/monocurl-essays/) demuestra cómo las escenas Monocurl pueden ejecutarse directamente en la web. El tiempo de ejecución subyacente también está disponible como [paquete NPM](https://www.npmjs.com/package/monocurl).

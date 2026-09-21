# Temas avanzados

Esta lección cubre temas que no son necesarios para la creación de escenas del día a día, pero que son útiles cuando desea personalizar el comportamiento de Monocurl o comprender lo que sucede bajo el capó.

## Bibliotecas personalizadas

Puede extraer definiciones comunes a un archivo de biblioteca. Un archivo de biblioteca está estructurado casi como un archivo de escena, pero sólo la parte de la biblioteca se importa a otros archivos. El objetivo principal de un archivo de biblioteca es proporcionar funciones auxiliares, operadores personalizados, constantes y definiciones de malla reutilizables que se pueden utilizar en múltiples escenas.

Puede importar su archivo de biblioteca a un archivo de escena con la palabra clave `import`. La ruta de importación es relativa al archivo de importación y no debe incluir la extensión `.mcs`.

Cuando se importa un archivo, Monocurl solo importa el código antes de la primera palabra clave `slide` de ese archivo. Cada diapositiva después de la primera `slide` se ignora a efectos de importación, incluidas las importaciones que aparecen después de esa primera diapositiva.

Esto permite que un archivo auxiliar mantenga una pequeña escena de demostración debajo de las definiciones de su biblioteca y es útil para editar el archivo de la biblioteca de forma aislada.

```mcl transcript
let Double = |x| 2 * x

slide "demo"
    print Double(4)
```

Otro archivo que lo importa puede usar `Double`, pero la diapositiva de demostración no se compila en la escena de importación.

## Valores y parámetros con estado

Los valores con estado son valores que dependen del estado de la escena y se actualizan continuamente. El lugar principal donde se encuentran los valores con estado son las superposiciones con reconocimiento de cámara. `camera_transfer{camera, $camera}` mantiene una malla fija en relación con el marco mientras la cámara se mueve (lea los documentos para obtener una explicación completa) y `orient_to_camera{$camera}` rota continuamente los árboles de malla planos hacia la cámara. Sin embargo, aparte de eso, se debe evitar el estado y es más idiomático hacer que las mallas tengan atributos que usted actualice explícitamente.

```mcl video
mesh fully_fixed =
    camera_transfer{camera, $camera}
    to_side{1u, 0.2}
    Text("fixed in frame")
mesh rotationally_fixed = 
    orient_to_camera{$camera}
    to_side{1d, 0.2}
    Text("rotationally fixed")
mesh not_fixed = Text("not fixed")

slide "Camera"
    camera = Camera([2, 1, 5])
    play CameraLerp(&camera, 1.2)
```

La fuente de estado es `param`, que funciona de manera similar a `mesh`: tiene un valor líder que edita el código y un valor seguidor hacia el que se reproduce la animación. Los parámetros de nivel superior también se exponen como controles interactivos en el modo de presentación.

Los parámetros más comunes son `camera` y `background`, que son un poco especiales ya que en realidad afectan la escena visual; otros parámetros se usan típicamente para controlar mallas.

Al leer un parámetro normalmente, como `radius`, se lee su valor líder actual. Leerlo con `$`, como `$radius`, crea una referencia con estado al valor en vivo. Puedes pensar en ello como si la expresión se reevaluara continuamente a medida que cambia el valor (aunque es más eficiente en la práctica).

Sólo puedes asignar valores con estado a las mallas. Hay operaciones limitadas que puedes realizar con ellos. Puede usarlos como argumentos de funciones, argumentos de operadores y en listas. También puede acceder a los atributos en la mayoría de los casos. Cuando asigna un valor con estado a una malla y sincroniza la malla a través de cualquier animación, la sincronización no es una sincronización "pura". En cambio, la malla líder se evaluará utilizando los parámetros del líder y la malla seguidora se evaluará utilizando el parámetro seguidor. Esto significa que al animar los parámetros, cambiará las mallas dependientes en pantalla.

Lo siguiente es una especie de ejemplo de juguete, ya que podría haberse hecho fácilmente con atributos, pero ilustra cómo se pueden usar parámetros/estado. El caso de uso más común es cuando muchas variables dependen naturalmente del estado de una escena o cuando desea cambiar los valores de los parámetros en el modo de presentación.
```mcl
param radius = 0.36

let Bubble = |radius, col|
    fill{alpha{0.2} col}
    stroke{col, 2}
    Circle(radius)

slide "Parameter"
    mesh bubble = Bubble(radius: $radius, col: BLUE)
    # referencing bubble nakedly uses the CURRENT value
    # so copy does not statefully depend on bubble!
    # try doing $bubble instead to see what happens
    mesh copy = bubble
    
    # the bubble leader depends on the radius leader
    # the bubble follower depends on the radius follower
    play Fade(0.4)

    # This edits the parameter leader.
    radius = 0.8
    # if you inspect bubble (leader) right now
    # it will be a circle

    # now we'll synchronize the radius follower
    # even though bubble was not changed since the last synchronization
    # its follower depends on the radius follower
    # so the bubble will change 
    # on the other hand, `copy` "elided" the stateful value
    # and no longer depends on radius so it remains the same
    play Lerp(1.0)
```

Finalmente, observamos que aunque el estado no puede realizar muchas operaciones (como la suma), puede ser el argumento de cualquier función. En el fondo, en cada reevaluación, la función se recupera con el valor actual del argumento con estado. Esto le permite evitar esta restricción, aunque de forma más detallada.

```mcl image

param radius = 2

mesh circle = fill{CLEAR} Circle($radius)
# won't work, can't do * on a stateful value
# mesh square = Square(2 * $radius)
# ... but you can use it as argument to any function
let double = |x| 2 * x
mesh square = fill{CLEAR} Square(double($radius))
```

## Látex avanzado

De forma predeterminada, Monocurl utiliza un backend LaTeX incluido. Si necesita la instalación de LaTeX en su sistema para paquetes o fuentes, la configuración del escritorio puede cambiar a un sistema personalizado `latex` más `dvisvgm` backend. La CLI tiene el indicador `--system-latex` coincidente. Esto le permite utilizar funciones en látex que no proporciona el paquete predeterminado.

Tenga en cuenta que, a diferencia de muchos otros lenguajes, Monocurl utiliza `%` como carácter de escape para cadenas en lugar de `\`, por lo que no es necesario utilizar dos barras invertidas de escape al escribir LaTeX.

`Text` es para texto literal. `Tex` es para fragmentos matemáticos ordinarios. `Latex` es para fragmentos de cuerpo LaTeX más completos y acepta un argumento `additional_preamble` para declaraciones de paquetes o fuentes.

`Tex` y `Latex` devuelven la geometría de la malla, por lo que se les puede aplicar estilo, etiquetar, filtrar y animar como otras mallas. Utilice `text_tag{...}` dentro de la entrada de texto cuando solo una parte de la expresión representada necesite una identidad estable.

```mcl
mesh eq = Tex([text_tag{1} "x", " + ", text_tag{2} "1"], 0.8)

slide "Equation"
    play Write(0.8, [&eq])

    eq = Tex([text_tag{2} "1", " + ", text_tag{1} "x"], 0.8)
    play TagTrans(1.0, [&eq])
```


Si una llamada `Tex(...)` no se procesa, la transcripción mostrará el resultado del compilador LaTeX. Las causas más comunes son la falta de paquetes y una sintaxis LaTeX no válida en el argumento de cadena.

## Operadores personalizados y cómo funcionan bajo el capó

Recuerde que los operadores son funciones que reciben un objetivo y devuelven un objetivo transformado. Están escritos antes del objeto sobre el que operan, lo que los hace buenos para tuberías de diseño y ubicación reutilizables.

```mcl
let soft_badge = operator |target, col|
    fill{alpha{0.18} col}
    stroke{col, 2}
    target

mesh markers = [
    center{1.2l} soft_badge{BLUE} Circle(0.35),
    center{1.2r} soft_badge{ORANGE} Square(0.6)
]
```

Una propiedad clave de los operadores es que puede variar entre `x` y `op{} x` para muchos operadores. Por ejemplo, lo siguiente es válido
```mcl
mesh org = Triangle(0l, 1u, 1r)
play Set()
org = rotate{180dg} org
play Lerp()
```

Si bien la mayoría de las veces puede definir sus propios operadores en términos de operadores stdlib, también puede crear operadores personalizados que tengan su propio comportamiento de interpolación. Un operador primitivo devuelve los valores "identidad" y "operado". El valor de identidad debe "verse" como el operando no modificado, pero contener atributos adicionales que permitan interpolarlo directamente con el valor "operado".

Por ejemplo, así es como se implementa `rotate`:
```mcl
let rotate = operator |target, radians, axis = 1b, pivot = nil, filter = nil| {
    let go = |angle| __monocurl__native__ op_rotate(target, angle, axis, pivot, filter)
    return [go(angle: 0), go(angle: radians)]
}
```
La rotación real se realiza mediante una función de óxido nativa para mayor eficiencia, pero el punto principal es que devolvemos dos valores. El primero gira por cero, que es el estado de identidad. El segundo gira la cantidad deseada. En la mayoría de los cálculos, el operador se trata simplemente como el segundo valor de retorno. Pero al hacer lerp entre `x` y `rotate{180dg} x`, Monocurl verá que este debería ser un operador lerp y observará el valor de identidad y "en realidad" lerp entre `go(angle: 0)` y `go(angle: 180dg)`, lo cual puede hacer mediante la interpolación tradicional.

## Animaciones primitivas

El modelo de animación se basa en la sincronización líder/seguidor. El código edita los líderes inmediatamente; `play` les dice a sus seguidores cómo ponerse al día.

El contenedor público de nivel más bajo es `PrimitiveAnim(time, &vars, embed, lerp, rate)`. Las animaciones de nivel superior, como `Lerp` y `Trans`, eventualmente se reducen a animaciones primitivas.

`PrimitiveAnim` especifica cómo se debe sincronizar el seguidor con el líder. La idea es que pueda proporcionar una función de interpolación personalizada que pueda diferir de lerp. Por ejemplo, así es como se define CameraLerp en stdlib.

```mcl
let CameraLerp = |&camera, time = 1, rate = smooth| {
    let embed = |start, dst| __monocurl__native__ camera_lerp_embed(start, dst)
    let value_lerp = |start, end, state, t| __monocurl__native__ camera_lerp_value(start, end, t)
    return PrimitiveAnim(time, &camera, embed, value_lerp, rate)
}
```

La mayor parte del trabajo pesado se realiza en Rust, pero aún podemos seguir el flujo general. La función de inserción preprocesa el inicio y el final y devuelve `[mod_start, mod_end, embed_state]`. Esto es útil para animaciones como Trans que necesitan realizar costosos algoritmos de coincidencia, por lo que preferiríamos tener que hacerlo solo una vez al principio. La función de interpolación recibe los argumentos de incrustación así como el valor t normalizado, y se le pide que interpola según el comportamiento deseado. En el caso de `CameraLerp`, esto equivale a realizar una interpolación esférica para que la visualización sea más natural.

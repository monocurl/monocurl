# Sujets avancés

Cette leçon couvre des sujets qui ne sont pas nécessaires à la création de scènes au quotidien, mais qui sont utiles lorsque vous souhaitez personnaliser le comportement de Monocurl ou comprendre ce qui se passe sous le capot.

## Bibliothèques personnalisées

Vous pouvez extraire des définitions communes dans un fichier de bibliothèque. Un fichier de bibliothèque est structuré presque comme un fichier de scène, mais seule la partie bibliothèque est importée dans d'autres fichiers. L'objectif principal d'un fichier de bibliothèque est de fournir des fonctions d'assistance, des opérateurs personnalisés, des constantes et des définitions de maillage réutilisables pouvant être utilisées dans plusieurs scènes.

Vous pouvez importer votre fichier bibliothèque dans un fichier de scène avec le mot-clé `import`. Le chemin d'importation est relatif au fichier d'importation et ne doit pas inclure l'extension `.mcs`.

Lorsqu'un fichier est importé, Monocurl importe uniquement le code avant le premier mot-clé `slide` de ce fichier. Chaque diapositive après le premier `slide` est ignorée à des fins d'importation, y compris toutes les importations qui apparaissent après cette première diapositive.

Cela permet à un fichier d'assistance de conserver une petite scène de démonstration en dessous de ses définitions de bibliothèque et est utile pour éditer le fichier de bibliothèque de manière isolée.

```mcl transcript
let Double = |x| 2 * x

slide "demo"
    print Double(4)
```

Un autre fichier qui l'importe peut utiliser `Double`, mais la diapositive de démonstration n'est pas compilée dans la scène d'importation.

## Valeurs et paramètres avec état

Les valeurs avec état sont des valeurs qui dépendent de l’état de la scène et sont continuellement mises à jour. Le principal endroit où les valeurs avec état sont les superpositions compatibles avec les caméras. `camera_transfer{camera, $camera}` maintient un maillage fixe par rapport au cadre pendant que la caméra se déplace (lisez la documentation pour une explication complète), et `orient_to_camera{$camera}` fait pivoter en continu les arbres de maillage planaire vers la caméra. Cependant, à part cela, le stateful doit être évité et il est plus idiomatique de faire en sorte que les maillages aient des attributs que vous mettez explicitement à jour.

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

La source de l'état est `param`, qui fonctionne de manière similaire à `mesh` : elle a une valeur leader que le code modifie et une valeur suiveuse vers laquelle l'animation est jouée. Les paramètres de niveau supérieur sont également exposés sous forme de contrôles interactifs en mode présentation.

Les paramètres les plus courants sont `camera` et `background`, qui sont un peu spéciaux car ils affectent réellement la scène visuelle, d'autres paramètres sont généralement utilisés pour contrôler les maillages.

La lecture normale d'un paramètre, tel que `radius`, lit sa valeur leader actuelle. Le lire avec `$`, tel que `$radius`, crée une référence avec état à la valeur active. Vous pouvez y penser comme si l'expression était continuellement réévaluée à mesure que la valeur change (même si c'est plus efficace en pratique).

Vous pouvez uniquement attribuer des valeurs avec état aux maillages. les opérations que vous pouvez effectuer sur eux sont limitées. Vous êtes autorisé à les utiliser comme arguments de fonction, arguments d'opérateur et dans des listes. Vous pouvez également accéder aux attributs dans la plupart des cas. Lorsque vous attribuez une valeur avec état à un maillage et synchronisez le maillage via une animation, la synchronisation n'est pas une synchronisation « pure ». Au lieu de cela, le maillage leader sera évalué à l'aide des paramètres de leader, et le maillage suiveur sera évalué à l'aide du paramètre suiveur. Cela signifie qu'en animant les paramètres, vous modifierez les maillages dépendants à l'écran.

Ce qui suit est en quelque sorte un exemple de jouet puisque cela aurait pu facilement être fait avec des attributs, mais illustre comment les paramètres/stateful peuvent être utilisés. Le cas d'utilisation le plus courant est celui où de nombreuses variables dépendent naturellement d'un état de scène ou lorsque vous souhaitez modifier les valeurs des paramètres en mode présentation.
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

Enfin, notons que même si stateful ne peut pas effectuer beaucoup d'opérations (comme l'addition), il peut être l'argument de n'importe quelle fonction. Sous le capot, à chaque réévaluation, la fonction est rappelée avec la valeur actuelle de l'argument avec état. Cela vous permet de contourner cette restriction, quoique de manière plus verbeuse.

```mcl image

param radius = 2

mesh circle = fill{CLEAR} Circle($radius)
# won't work, can't do * on a stateful value
# mesh square = Square(2 * $radius)
# ... but you can use it as argument to any function
let double = |x| 2 * x
mesh square = fill{CLEAR} Square(double($radius))
```

## LaTeX avancé

Par défaut, Monocurl utilise un backend LaTeX fourni. Si vous avez besoin de l'installation de LaTeX sur votre système pour les packages ou les polices, les paramètres du bureau peuvent basculer vers un système personnalisé `latex` plus un backend `dvisvgm`. La CLI a l’indicateur `--system-latex` correspondant. Cela vous permet d'utiliser des fonctionnalités en latex non fournies par le bundle par défaut.

Veuillez noter que contrairement à de nombreux autres langages, Monocurl utilise `%` comme caractère d'échappement pour les chaînes au lieu de `\`, vous n'avez donc pas besoin de doubler les barres obliques inverses d'échappement lors de l'écriture de LaTeX.

`Text` est pour le texte littéral. `Tex` est destiné aux fragments mathématiques ordinaires. `Latex` est destiné aux fragments de corps LaTeX plus complets et accepte un argument `additional_preamble` pour les déclarations de package ou de police.

`Tex` et `Latex` renvoient la géométrie du maillage, afin qu'ils puissent être stylisés, étiquetés, filtrés et animés comme les autres maillages. Utilisez `text_tag{...}` dans la saisie de texte lorsque seule une partie de l'expression rendue a besoin d'une identité stable.

```mcl
mesh eq = Tex([text_tag{1} "x", " + ", text_tag{2} "1"], 0.8)

slide "Equation"
    play Write(0.8, [&eq])

    eq = Tex([text_tag{2} "1", " + ", text_tag{1} "x"], 0.8)
    play TagTrans(1.0, [&eq])
```


Si un appel `Tex(...)` échoue, la transcription affichera la sortie du compilateur LaTeX. Les causes les plus courantes sont des packages manquants et une syntaxe LaTeX invalide dans l'argument de chaîne.

## Opérateurs personnalisés et comment ils fonctionnent sous le capot

Rappelons que les opérateurs sont des fonctions qui reçoivent une cible et renvoient une cible transformée. Ils sont écrits avant l'objet sur lequel ils opèrent, ce qui les rend parfaits pour les pipelines de style et de placement réutilisables.

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

Une propriété clé des opérateurs est que vous pouvez lire entre `x` et `op{} x` pour de nombreux opérateurs. Par exemple, ce qui suit est valide
```mcl
mesh org = Triangle(0l, 1u, 1r)
play Set()
org = rotate{180dg} org
play Lerp()
```

Bien que la plupart du temps, vous puissiez définir vos propres opérateurs en termes d'opérateurs stdlib, vous pouvez également créer des opérateurs personnalisés dotés de leur propre comportement d'interpolation. Un opérateur primitif renvoie les valeurs « identité » et « opéré ». La valeur d'identité doit "ressembler" à l'opérande non modifié, mais contenir des attributs supplémentaires qui lui permettent d'être interpolée directement avec la valeur "opérée".

Par exemple, voici comment `rotate` est implémenté :
```mcl
let rotate = operator |target, radians, axis = 1b, pivot = nil, filter = nil| {
    let go = |angle| __monocurl__native__ op_rotate(target, angle, axis, pivot, filter)
    return [go(angle: 0), go(angle: radians)]
}
```
La rotation réelle est effectuée par une fonction rust native pour plus d'efficacité, mais le point principal est que nous renvoyons deux valeurs. Le premier tourne de zéro, ce qui correspond à l’état d’identité. La seconde tourne du montant souhaité. Dans la plupart des calculs, l'opérateur est simplement traité comme la deuxième valeur du rendement. Mais lors d'une lecture entre `x` et `rotate{180dg} x`, Monocurl verra qu'il devrait s'agir d'un opérateur lerp et examinera la valeur d'identité et "en fait" lerp entre `go(angle: 0)` et `go(angle: 180dg)`, ce qu'il peut faire via une interpolation traditionnelle.

## Animations primitives

Le modèle d'animation est construit sur la synchronisation leader/suiveur. Le code modifie immédiatement les dirigeants ; `play` indique aux abonnés comment rattraper leur retard.

Le wrapper public de niveau le plus bas est `PrimitiveAnim(time, &vars, embed, lerp, rate)`. Les animations de niveau supérieur, telles que `Lerp` et `Trans`, finissent par se réduire à des animations primitives.

`PrimitiveAnim` spécifie comment le suiveur doit être synchronisé avec le leader. L'idée est que vous pouvez fournir une fonction d'interpolation personnalisée qui peut différer de lerp. Par exemple, voici comment CameraLerp est défini dans stdlib.

```mcl
let CameraLerp = |&camera, time = 1, rate = smooth| {
    let embed = |start, dst| __monocurl__native__ camera_lerp_embed(start, dst)
    let value_lerp = |start, end, state, t| __monocurl__native__ camera_lerp_value(start, end, t)
    return PrimitiveAnim(time, &camera, embed, value_lerp, rate)
}
```

La majeure partie du gros du travail est effectuée à Rust, mais nous pouvons toujours parcourir le flux général. La fonction embed prétraite le début et la fin, renvoyant `[mod_start, mod_end, embed_state]`. Ceci est utile pour les animations comme Trans qui nécessitent des algorithmes de correspondance coûteux, nous préférerions donc ne le faire qu'une seule fois au début. La fonction d'interpolation reçoit les arguments d'embed ainsi que la valeur t normalisée, et il lui est demandé d'interpoler en fonction du comportement souhaité. Dans le cas de `CameraLerp`, cela revient à effectuer une interpolation sphérique pour rendre la visualisation plus naturelle.

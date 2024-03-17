# Monocurl (beta v0.1.0)
<img width="1312" alt="monocurl" src="https://github.com/monocurl/monocurl/assets/43832426/d9be28e4-9916-4b18-949a-1b1f9b5d5530">

![image](https://github.com/monocurl/monocurl/assets/43832426/ec18e407-31cb-4e6d-ae50-c44e4cef668d)
![image](https://github.com/monocurl/monocurl/assets/43832426/2df89fb8-2e8e-439c-b1d9-7119884ba10e)
![image](https://github.com/monocurl/monocurl/assets/43832426/031e366d-7a8b-4b0a-a4c4-60ecf0973324)
![image](https://github.com/monocurl/monocurl/assets/43832426/689e05fa-1b7d-41e2-b43a-f87f4e939fd7)
![image](https://github.com/monocurl/monocurl/assets/43832426/29c5bd64-9875-48de-a90e-fa00f3adaa81)
![image](https://github.com/monocurl/monocurl/assets/43832426/76fc0d65-a04c-459e-b7ea-50aaa450d0b7)
![image](https://github.com/monocurl/monocurl/assets/43832426/05821b7d-f89d-465c-b8e7-1d1a0f986ac4)


*Make videos and slideshow presentations using math*

Monocurl is a scripting language and a desktop application used to create STEM videos and slideshow. The core idea of Monocurl is to combine the benefits of programmatic animations with the traditional feel of a video editor. 

## Download
The beta edition for macOS and Windows is available on our [website](https://www.monocurl.com/). Please give feedback in our [Discord](https://discord.com/invite/7g94JR3SAD) server!

## Minimal Working Example

```
tree circ = Circle:
    center: ORIGIN
    radius: 1
    tag: {}
    color: default

p += Set:
    vars&: circ

circ = {}
p += Fade:
    meshes&: circ
    time: 1
```

Result:

![minimal_example](https://github.com/monocurl/monocurl/assets/43832426/bded8e5b-1fb0-4617-9b33-0af252ab6d49)


## High Level Idea

Monocurl combines the idea of keyframes with programmatic animation. In particular, there exists the idea of the iterator and the follower. The iterator is a normal variable that cycles through different key states (e.g. `circ` in the above example). We then tell the follower how to interpolate between the different keyframes using animations (e.g. `Set` and `Fade`). This provides our basis for complex projects.

## Resources

We have a tutorial series available on our [website](https://www.monocurl.com/learn/0_What_is_Monocurl) (and accompany videos on [YouTube](https://www.youtube.com/@monocurl) as well).

Please feel free to ask for help (or contribute ideas) in our [Discord](https://discord.com/invite/7g94JR3SAD) server!

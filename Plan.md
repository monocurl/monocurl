# Plan

## System Design
- main (ui) thread
- lexing thread
- parsing / compilation / auto completion thread
- execution thread + worker thread pool
- server / coordinator shared component
  - coordinates differently during rendering?
  - or just launch a new orchestrator configured with no watcher, but with the necessary components

## Orchestrator
- can be configured with multiple components
  - export scheduler (for exporting)
  - export renderer
  - file watcher (for live)
  - lexing / parsing threads
- maintains editor state

- we have a combination of shared state (when needed to be accessed by multiple) and owned state
- maybe instead of one giant state object, we have editor containing multiple components, each of which contain their own state?
  - much nicer


## Lexer
-

## Parser
-

## Compiler
-

## Executor
- Latex / Media will be difficult
- runtime will also be a bit difficult

## Standard Library
- comp geo should

## Autocomplete
- separate thread

## File Watcher
-

## CLI
- should honestly be pretty simple wrapper?

## UI
- Code (main)
- Rendering
- Timeline should be simple
- Parameters / Presentation

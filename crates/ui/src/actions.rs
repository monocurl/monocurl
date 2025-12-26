use gpui::actions;

actions!(
    app,
    [Quit]
);

actions!(
    document,
    [SaveActiveDocumentCustomPath, SaveActiveDocument, CloseActiveDocument]
);

actions!(
    text_input,
    [
        Backspace,
        Delete,
        BackspaceWord,
        BackspaceLine,
        Enter,
        Up,
        Left,
        Right,
        Down,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectAll,
        Home,
        End,
        ShowCharacterPalette,
        Paste,
        Cut,
        Copy,
    ]
);

actions!(
    editor,
    [
        Undo,
        Redo,
        TogglePresentationMode,
        TogglePlaying,
        SceneStart,
        SceneEnd,
        PrevSlide,
        NextSlide,
        EpsilonForward,
        EpsilonBackward,
    ]
);

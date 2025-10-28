use gpui::actions;

actions!(
    app,
    [Quit]
);

actions!(
    document,
    [SaveActiveDocument, CloseActiveDocument]
);

actions!(
    text_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
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
        ToggleTextEditor,
        SceneStart,
        SceneEnd,
        PrevSlide,
        NextSlide,
        EpsilonForward,
        EpsilonBackward,
    ]
);

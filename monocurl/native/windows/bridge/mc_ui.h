#pragma once

using namespace System::Windows;
using namespace System;
using namespace System::Collections::Generic;


struct raw_media_model;
struct timeline;

namespace Bridge
{

    public ref class Media : public Forms::Control
    {
        Forms::Button^ relink;
        Forms::Label^ nameLabel;
        Forms::Label^ pathLabel;

        raw_media_model* media;

        void OnClick(Object^ sender, EventArgs^ e);

    public:
        Media();

        void SetMedia(raw_media_model* media);
        void SetName(char const* name);
        void SetPath(char const* path);
    };

    extern "C" LRESULT SendMessageW(HWND hWnd, UINT Msg, WPARAM wParam, LPARAM lParam);

    public ref class MTextBox : public Forms::TextBox
    {
    protected:
        virtual void WndProc(Forms::Message% m) override {
            /* WM_MOUSEWHELL */
            if (m.Msg == 0x020A) {
				SendMessage(
					reinterpret_cast<HWND>(Parent->Handle.ToPointer()),
					m.Msg,
					(WPARAM)m.WParam.ToPointer(),
					(LPARAM)m.LParam
				);
				return;
            }

            TextBox::WndProc(m);
        }
    };

    public ref class Slide : public Forms::Panel
    {
        Forms::Label^ title;
        MTextBox^ content;
        Forms::Button^ add_button;
        Forms::Button^ delete_button;
        Forms::Label^ error;
        Forms::ToolTip^ tooltip;

        Stack<String^>^ prev;
        Stack<int>^ prev_loc;
        Stack<String^>^ next;
        Stack<int>^ next_loc;

        List<int>^ functor_start;
        List<int>^ functor_end;

        struct raw_slide_model* cache;

        void AddSlide(System::Object^ sender, EventArgs^ args);
        void DeleteSlide(System::Object^ sender, EventArgs^ args);

        void Slide_onClick(System::Object^ sender, EventArgs^ args);
        void Text_previewKeyPress(System::Object^ sender, Forms::PreviewKeyDownEventArgs^ args);

        bool CanEdit(Forms::KeyEventArgs^ key);
        void Text_Clicked(System::Object^ sender, Forms::MouseEventArgs^ args);
        void Text_keyPress(System::Object^ sender, Forms::KeyEventArgs^ args);
        void Text_onChanged(System::Object^ sender, EventArgs^ args);
        void Text_resize(System::Object^ sender, EventArgs^ args);

        void Indent(int delta);
        void NextLine(void);
        void PrevLine(void);
        int SelectLine(int cursor);

        void AdjustRect(void);

        const int _EM_SETTABSTOPS = 0x00CB;
        void SetTabWidth(Forms::TextBox^ textbox, int tabWidth);

    public:
        Slide();
        void SaveUndo(void);
        void Update(raw_slide_model* slide);
    };


    // https://blog.walterlv.com/post/handle-horizontal-scrolling-of-touchpad-en.html
    // at some point... no idea why it's so difficult
    public ref class Timeline : public Forms::Control {

        ref class TimelinePanel : public Forms::Panel {

        public:
            void expose_proc(Forms::Message% m) {
                this->WndProc(m);
            }
        };

        // not a race condition (?) since only accesible by main thread
        bool inClosingMode{ false }; // in which case cancel all updates;

        struct timeline* timeline;
        double cacheOffset;
        size_t cacheSlideNum; // for use with seeking.
        size_t cachedSlideCount; // for use with seeking

        Forms::Panel^ toolbar;

        Forms::Button^ playButton;
        Forms::Button^ fastBackward;
        Forms::Button^ superFastBackward;

        Forms::Label^ timestamp;

        Forms::Button^ exportButton;

        TimelinePanel^ mainTimeline;
        Forms::Panel^ slides;
        Forms::Panel^ cursorAnchor;
        Forms::Panel^ cursor;

        void Play_onClick(System::Object^ sender, System::EventArgs^ args);

        void SuperBackward_onClick(System::Object^ sender, System::EventArgs^ args);
        void FastBackward_onClick(System::Object^ sender, System::EventArgs^ args);

        void Export_onClick(System::Object^ sender, System::EventArgs^ args);

        void Timeline_onClick(System::Object^ sender, EventArgs^ args);

    protected:
        virtual void WndProc(Forms::Message% m) override {
            /* WM_MOUSEHWHEEL */
            if (m.Msg == 0x020E) {
                Forms::Message% m2 = m;
                m2.Msg = 0x020A;
                m2.WParam = (IntPtr) -m.WParam.ToInt64();
                mainTimeline->expose_proc(m2);

				return;
            }

            Control::WndProc(m);
        }

    public:
        Timeline(struct timeline* timeline, Forms::Panel^ container);

        void ToggleInClosingMode(void);
        void Update(void);

        void PrevSlide(void);
        void NextSlide(void);
        void SceneStart(void);
        void SceneEnd(void);

        void TogglePlay(void);
    };

}

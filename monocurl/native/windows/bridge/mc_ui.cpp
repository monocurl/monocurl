extern "C" {
#include "monocurl.h"
#include "slide.h"
}

#include "mc_ui.h"
#include "mc_bridge.h"
#include "mc_viewport.h"
#include "mc_scene_bridge.h"
#include "mc_exporter.h"

using namespace Forms;
using namespace System::Drawing;

static void set_click_handler(Control^ control, EventHandler^ handler)
{
    for (int i = 0; i < control->Controls->Count; ++i) {
        Control^ child = control->Controls[i];
        set_click_handler(child, handler);
    }
	control->Click += handler;
}

ref class MediaRelink : public Form
{
private:
    TextBox^ nameTextBox;
    Label^ pathTextBox;
    Button^ confirmButton;
    Button^ cancelButton;
    Button^ deleteButton;
    Button^ pickPathButton;

    raw_media_model* media;

public:
    MediaRelink(raw_media_model* media) : media{ media }
    {

        InitializeComponents();
    }

private:
    void InitializeComponents()
    {
        nameTextBox = gcnew TextBox();
        pathTextBox = gcnew Label();
        confirmButton = gcnew Button();
        cancelButton = gcnew Button();
        deleteButton = gcnew Button();

        nameTextBox->Text = gcnew String(media->name);
        nameTextBox->Location = Drawing::Point(20, 20);
        nameTextBox->Size = Drawing::Size(250, 20);

        pathTextBox->Text = gcnew String(media->path);
        pathTextBox->Location = Drawing::Point(20, 50);
        pathTextBox->Size = Drawing::Size(250, 20);
        
        pickPathButton = gcnew Button();
        pickPathButton->Text = "Path";
        pickPathButton->Location = Drawing::Point(300, 50);
        pickPathButton->AutoSize = true;
        pickPathButton->Click += gcnew EventHandler(this, &MediaRelink::PickPathButton_Click);

        confirmButton->Location = Drawing::Point(100, 140);
        confirmButton->AutoSize = true;
        confirmButton->Text = "Confirm";
        confirmButton->Click += gcnew EventHandler(this, &MediaRelink::ConfirmButton_Click);

        cancelButton->Location = Drawing::Point(20, 140);
        cancelButton->AutoSize = true;
        cancelButton->Text = "Cancel";
        cancelButton->Click += gcnew EventHandler(this, &MediaRelink::CancelButton_Click);

        deleteButton->Location = Drawing::Point(300, 140);
        deleteButton->AutoSize = true;
        deleteButton->Text = "Delete";
        deleteButton->Click += gcnew EventHandler(this, &MediaRelink::DeleteButton_Click);

        Controls->Add(nameTextBox);
        Controls->Add(pathTextBox);
        Controls->Add(pickPathButton);
        Controls->Add(confirmButton);
        Controls->Add(cancelButton);
        Controls->Add(deleteButton);

        Text = "Media Relink";
        Width = 420;
        Height = 230;

        FormBorderStyle = Forms::FormBorderStyle::FixedDialog;
    }

    void PickPathButton_Click(Object^ sender, EventArgs^ e)
    {
        OpenFileDialog^ openFileDialog = gcnew OpenFileDialog();
        openFileDialog->Title = "Select Media Path";
        openFileDialog->Filter = "Image Files|*.jpg;*.jpeg;*.png;";
        openFileDialog->CheckFileExists = true;
        openFileDialog->FileName = pathTextBox->Text;

        if (openFileDialog->ShowDialog() == Forms::DialogResult::OK)
        {
            pathTextBox->Text = openFileDialog->FileName;
        }
    }

    void ConfirmButton_Click(Object^ sender, EventArgs^ e)
    {
        String^ name = nameTextBox->Text;
        String^ path = pathTextBox->Text;

        media_switch_path(media, Bridge::cstring_for(path));
        media_switch_name(media, Bridge::cstring_for(name));

        Close();
    }

    void CancelButton_Click(Object^ sender, EventArgs^ e)
    {
        Close();
    }

    void DeleteButton_Click(Object^ sender, EventArgs^ e)
    {
        Forms::DialogResult result = MessageBox::Show("Are you sure you want to delete?", "Confirmation", MessageBoxButtons::YesNo, MessageBoxIcon::Question);

        if (result == Forms::DialogResult::Yes)
        {
			media_delete(media);
            Close();
        }
    }
};

Bridge::Media::Media() {
	Size = Drawing::Size(200, 40);
	Dock = DockStyle::Top;

	DoubleBuffered = true; 

	relink = gcnew Button();
	relink->AutoSize = true;
    relink->BackColor = Color::White;
	relink->Text = "Update";
	relink->Location = Drawing::Point(5, 5);
	relink->Click += gcnew System::EventHandler(this, &Media::OnClick);
	Controls->Add(relink);

	nameLabel = gcnew Forms::Label();
    nameLabel->ForeColor = Color::White;
	nameLabel->AutoSize = true;
	nameLabel->Location = Drawing::Point(85, 10);
	Controls->Add(nameLabel);
	
	pathLabel = gcnew Forms::Label();
    pathLabel->ForeColor = Color::White;
	pathLabel->AutoSize = true;
	pathLabel->Location = Drawing::Point(175, 10);
	Controls->Add(pathLabel);
}
    
void Bridge::Media::OnClick(Object^ sender, EventArgs^ e) {
    MediaRelink^ relink = gcnew MediaRelink(media);
    relink->ShowDialog();
}

void Bridge::Media::SetMedia(raw_media_model* media) {
    this->media = media;
}

void Bridge::Media::SetName(char const* name) {
	if (!streq(name, nameLabel->Text)) {
		nameLabel->Text = gcnew System::String(name);
	}	
}

void Bridge::Media::SetPath(char const* path) {
	if (!streq(path, pathLabel->Text)) {
		pathLabel->Text = gcnew System::String(path);
	}	
}

ref class FunctorGroup : Forms::Panel {
    // line
    // left button, right button
    Panel^ line;
    Label^ left;
    Label^ right;

    int curr_line, curr_lines, curr_tabs;
    String^ next_string, ^prev_string;
    List<String^> ^next, ^prev;
    Bridge::Slide^ up;

public:
    FunctorGroup() {
        line = gcnew Panel();
        line->BackColor = Color::Gray;
        line->Width = 2;    
        line->Left = 34;
        line->Anchor = AnchorStyles::Top | AnchorStyles::Bottom;

        left = gcnew Label();
        left->Text = "<";
        left->Width = 13;
        left->Left = 7;
        left->Click += gcnew System::EventHandler(this, &FunctorGroup::MinusClick);

        right = gcnew Label();
        right->Text = ">";
        right->Width = 13;
        right->Left = 18;
        right->Click += gcnew System::EventHandler(this, &FunctorGroup::PlusClick);

        next = gcnew List<String^>();
        prev = gcnew List<String^>();

        this->Width = 36;
        this->BackColor = Color::Transparent;
        this->DoubleBuffered = true;
    }

    void replace(List<String^>^ ref, String^ fallback) {
        TextBox^ parent = dynamic_cast<TextBox^>(this->Parent);

        // save undo
        up->SaveUndo();

        int curr_pos = 0, lines = 0;
        while (curr_pos < parent->TextLength && lines < curr_line) {
            if (parent->Text[curr_pos] == '\n') {
                lines++;
            }
            curr_pos++;
        }

        // delete
        List<String^>^ build = gcnew List<String^>();
        int it = curr_pos;
        lines = 0;
        bool start = false;
        String^ running = "";
        while (it < parent->TextLength && lines < this->curr_lines) {
            if (parent->Text[it] == ':') {
                start = true;
            }
            else if (parent->Text[it] == '\n') {
                running += "\n";
                build->Add(running);
                running = "";
                start = false;
                lines++;
				it++;
                continue;
            }

            if (start) {
                running += parent->Text[it];
            }

            it++;
        }
        // replace
        String^ insert = "";
        for (int i = 0; i < ref->Count; ++i) {
            for (int j = 0; j < curr_tabs; ++j) {
                insert += "\t";
			}
            insert += ref[i];
            if (i < build->Count) {
                insert += build[i];
            } 
            else {
				insert += ": \r\n";
            }
        }
        if (!ref->Count) {
            for (int j = 0; j < curr_tabs; ++j) {
                insert += "\t";
			}
            insert += fallback;
        }

        parent->Text = parent->Text
            ->Remove(curr_pos, it - curr_pos)
            ->Insert(curr_pos, insert);
        parent->SelectionStart = curr_pos + insert->Length - 2;
        parent->SelectionLength = 0;
    }

    void PlusClick(System::Object^ obj, EventArgs ^ e) {
        replace(next, next_string);
    }

    void MinusClick(System::Object^ obj, EventArgs ^ e) {
        replace(prev, prev_string);
    }

    void Update(Bridge::Slide^ up, raw_slide_model::slide_functor_group g) {
        this->up = up;
        this->curr_line = (int) g.line;
        this->curr_lines = (int) g.modes[g.current_mode].arg_count;
        this->curr_tabs = (int) g.tabs;

        if (g.mode_count > 1) {
			this->prev->Clear();
			this->next->Clear();

            auto l = g.modes[(g.current_mode + g.mode_count - 1) % g.mode_count];
            for (int i = 0; i < l.arg_count; ++i) {
                this->prev->Add(gcnew String(l.arg_titles[i]));
            }
            if (!l.arg_count) {
                prev_string = gcnew String(g.title) + ": " + gcnew String(l.title) + "\r\n";
            }

            l = g.modes[(g.current_mode + 1) % g.mode_count];
            for (int i = 0; i < l.arg_count; ++i) {
                this->next->Add(gcnew String(l.arg_titles[i]));
            }
            if (!l.arg_count) {
                next_string = gcnew String(g.title) + ": " + gcnew String(l.title) + "\r\n";
            }

			this->Controls->Add(right);
			this->Controls->Add(left);
		}
        else {
			this->Controls->Remove(left);
			this->Controls->Remove(right);
        }

        if (g.modes[g.current_mode].arg_count > 1 || g.mode_count > 1) {
            this->Controls->Add(this->line);
        }
        else {
            this->Controls->Remove(this->line);
        }

        this->Left = (int) (g.tabs - 1) * this->Width;
        this->Height = (int) max(1, g.modes[g.current_mode].arg_count) * 20 - 2;
        this->Top = (int) g.line * 20 + 1;
    }
};

Bridge::Slide::Slide() {
    this->title = gcnew Label();
    this->title->Dock = DockStyle::Top;
    this->title->TextAlign = ContentAlignment::MiddleCenter;
    this->title->Size = Drawing::Size(100, 30);
    this->title->ForeColor = Color::White;

    this->content = gcnew MTextBox();
    this->content->Multiline = true;
    this->content->Font = gcnew System::Drawing::Font("Consolas", 10);
    this->content->Dock = DockStyle::Bottom;
    this->content->WordWrap = false;
    this->content->KeyDown += gcnew System::Windows::Forms::KeyEventHandler(this, &Slide::Text_keyPress);
    this->content->PreviewKeyDown += gcnew System::Windows::Forms::PreviewKeyDownEventHandler(this, &Slide::Text_previewKeyPress);
    this->content->TextChanged += gcnew System::EventHandler(this, &Slide::Text_onChanged);;
    this->content->Resize += gcnew System::EventHandler(this, &Slide::Text_resize);
    this->content->BackColor = Drawing::Color::FromArgb(255, 38, 38, 38);
    this->content->ForeColor = Drawing::Color::White;
    this->content->MouseClick += gcnew System::Windows::Forms::MouseEventHandler(this, &::Bridge::Slide::Text_Clicked);
    SetTabWidth(this->content, 4);

    this->add_button = gcnew Button();
    this->add_button->Text = "New Slide";
    this->add_button->Size = Drawing::Size(100, 30);
    this->add_button->FlatStyle = FlatStyle::Flat;
    this->add_button->ForeColor = Color::LightBlue;
    this->add_button->FlatAppearance->BorderSize = 0;
    this->add_button->Dock = DockStyle::Bottom;
    this->add_button->Click += gcnew System::EventHandler(this, &Slide::AddSlide);

    this->delete_button = gcnew Button();
    this->delete_button->Dock = DockStyle::Right;
    this->delete_button->Size = Drawing::Size(80, 30);
    this->delete_button->FlatStyle = FlatStyle::Flat;
    this->delete_button->ForeColor = Color::OrangeRed;
    this->delete_button->FlatAppearance->BorderSize = 0;
    this->delete_button->Text = "delete";
    this->delete_button->Click += gcnew System::EventHandler(this, &Slide::DeleteSlide);

    this->title->Controls->Add(this->delete_button);

    this->error = gcnew Label();
    this->error->TextAlign = ContentAlignment::MiddleCenter;
    this->error->BackColor = Color::Red;
    this->error->ForeColor = Color::Black;
    this->error->Text = "!";
    this->error->Width = 23;
    this->error->Height = 20;
    this->error->Anchor = AnchorStyles::Top | AnchorStyles::Right;
    content->Controls->Add(error);

    this->tooltip = gcnew ToolTip();

    this->Controls->Add(this->title);
    this->Controls->Add(this->content);
    this->Controls->Add(this->add_button);

    this->Dock = DockStyle::Top;
    this->DoubleBuffered = true;

    this->AutoSize = true;            
    this->AutoSizeMode = Forms::AutoSizeMode::GrowAndShrink;

    this->prev = gcnew Stack<String^>();
    this->prev_loc = gcnew Stack<int>();
    this->next = gcnew Stack<String^>();
    this->next_loc = gcnew Stack<int>();

    this->functor_start = gcnew List<int>();
    this->functor_end = gcnew List<int>();

    EventHandler^ eh = gcnew System::EventHandler(this, &::Bridge::Slide::Slide_onClick);
    this->Click += eh;
    this->title->Click += eh;
}

#pragma message("TODO, technically there's a race condition on slide_write_error")
void Bridge::Slide::Update(raw_slide_model* slide) {
    this->cache = slide;

    mc_ind_t const index = slide_index_in_parent(slide);
    if (!index) {
        this->title->Text = "Config";
    }
    else {
        this->title->Text = gcnew String("Slide ") + index;
    }

    if (index == slide->scene->slide_count - 1) {
        this->Padding = Forms::Padding(0, 0, 0, 400);
    }
    else {
        this->Padding = Forms::Padding(0, 0, 0, 40);
    }

    this->functor_start->Clear();
    this->functor_end->Clear();
    for (int i = 0; i < slide->total_functor_args; ++i) {
        functor_start->Add((int) slide->functor_arg_start[i]);
        functor_end->Add((int) slide->functor_arg_end[i]);
    }

    bool deletable = index > 1 || index == 1 && slide->scene->slide_count > 2;
    if (!deletable && this->title->Controls->Count) {
        this->title->Controls->Remove(this->delete_button);
    }
    else if (deletable && !this->title->Controls->Count) {
        this->title->Controls->Add(this->delete_button);
    }

    String^ next = (gcnew String(slide->buffer))->ReplaceLineEndings();
    if (next != this->content->Text) {
        int start = this->content->SelectionStart;
		this->content->Text = next;
        this->content->SelectionStart = start;
    }

    if (slide->error.message) {
        this->tooltip->SetToolTip(error, gcnew String(slide->error.message));
		this->error->Left = this->Width - 35;
        this->error->Top = (int) slide->error.line * 20;
        this->error->Height = 20;
    }
    else {
        this->error->Height = 0;
    }

    int used_count = (int) cache->group_count;
    for (int i = 0; i < index; ++i) {
        raw_slide_model* s = slide->scene->slides[i];
        if (s->error.message && s->error.type == raw_slide_model::slide_error::SLIDE_ERROR_SYNTAX) {
            used_count = 0;
            break;
        }
    }

    content->SuspendLayout();
    adjust_control_list<FunctorGroup>(content->Controls, used_count + 1);
    for (int i = 0; i < used_count; ++i) {
        dynamic_cast<FunctorGroup^>(content->Controls[i + 1])->Update(this, cache->functor_groups[i]);
    }
    content->ResumeLayout();

    // \r offset
    for (int i = 0, l = 0, j = 0; i < content->TextLength; ++i) {
        if (content->Text[i] == '\r') {
            ++l;
        }

        while (j < functor_start->Count && i - l > functor_start[j]) {
            ++j;
        }

        if (j < functor_start->Count && functor_start[j] == i - l) {
            functor_start[j] += l;
            functor_end[j] += l;
            j++;
        }
    }
}

void Bridge::Slide::AddSlide(System::Object^ sender, EventArgs^ args) {
    insert_slide_after(this->cache);
}

void Bridge::Slide::DeleteSlide(System::Object^ sender, EventArgs^ args) {
    Forms::DialogResult result = MessageBox::Show("Are you sure you want to delete the slide? This action is undoable", "Delete Slide?", MessageBoxButtons::YesNo, MessageBoxIcon::Question);

    if (result == Forms::DialogResult::Yes)
    {
		delete_slide(this->cache);
    }
}

void Bridge::Slide::Slide_onClick(System::Object^ sender, EventArgs^ args) {
    this->FindForm()->ActiveControl = nullptr;
}

void Bridge::Slide::Text_previewKeyPress(System::Object^ sender, Forms::PreviewKeyDownEventArgs^ e)
{
    if (e->KeyCode == Keys::Tab) {
        e->IsInputKey = true;
    }
}

bool Bridge::Slide::CanEdit(Forms::KeyEventArgs^ ev)
{
    Keys key = ev->KeyCode;
    if (key == Keys::Up || key == Keys::Down || key == Keys::Right || key == Keys::Left) {
        return true;
    }

    int s = content->SelectionStart, e = s + content->SelectionLength;
    if (ev->KeyData == Keys::Back || ev->KeyData == (Keys::Back | Keys::ControlKey) || ev->KeyData == (Keys::Back | Keys::Control)) {
        if (e == s && s) {
            s--;
        }
    }

    // if entirely contained, stop
    // if partially contained, clip
    for (int i = 0; i < functor_start->Count; ++i) {
        int u = functor_start[i];
        int v = functor_end[i];
        if (u >= e) {
            break;
        }
        if (s >= u && s < v && e <= v) {
            return false;
        }
        else if (s <= u && e >= u && e < v) {
            this->content->SelectionLength = max(0, this->content->SelectionLength - (e - u));
            return true;
        }
        else if (s > u && s < v && e > v) {
            this->content->SelectionStart += v - s;
            this->content->SelectionLength = max(0, this->content->SelectionLength - (v - s));
            return true;
        }
    }

    return true;
}

void Bridge::Slide::Text_keyPress(System::Object^ sender, Forms::KeyEventArgs^ e)
{
    if (e->KeyCode == Keys::Escape)
    {
        this->FindForm()->ActiveControl = nullptr;
    }
    else if (e->Control && e->KeyCode == Keys::Z) {
        while (this->prev->Count) {
            String^ old = this->content->Text;
            this->next->Push(this->content->Text);
            this->next_loc->Push(this->content->SelectionStart);
            this->content->Text = this->prev->Pop();
            this->content->SelectionStart = this->prev_loc->Pop();
            if (old != this->content->Text) {
                break;
            }
        }
    }
    else if (e->Control && e->Shift && e->KeyCode == Keys::Z) {
        while (this->next->Count) {
            String^ old = this->content->Text;
			this->prev->Push(this->content->Text);
			this->prev_loc->Push(this->content->SelectionStart);
			this->content->Text = this->next->Pop();
			this->content->SelectionStart = this->next_loc->Pop();
            if (old != this->content->Text) {
                break;
            }
        }
    }
    else if (CanEdit(e)) {
        this->SaveUndo();
    }
    else {
        e->SuppressKeyPress = true;
    }

	if (e->KeyCode == Keys::Tab) {
		if (e->Shift) {
            PrevLine();
		}
		else {
            NextLine();
		}
        e->Handled = true;
        e->SuppressKeyPress = true;
    }
	else if (e->KeyCode == Keys::OemCloseBrackets && e->Control) {
		Indent(1);
        e->Handled = true;
        e->SuppressKeyPress = true;
	}
	else if (e->KeyCode == Keys::OemOpenBrackets && e->Control) {
		Indent(-1);
        e->Handled = true;
        e->SuppressKeyPress = true;
	}
    else if (e->KeyCode == Keys::Enter) {
        e->SuppressKeyPress = true;
        e->Handled = true;

        int it = this->content->SelectionStart;
        int tabs = it < this->content->TextLength ? this->content->Text[it] == '\t' : 0;
        while (it > 0 && this->content->Text[it - 1] != '\n') {
            if (this->content->Text[--it] == '\t') {
                tabs++;
            }
        }
        it = this->content->SelectionStart;
        while (it < this->content->TextLength && this->content->Text[it] != '\n') {
            it++;
        }
        it++;
        int nextTabs = 0;
        if (this->content->SelectionStart > 0 && this->content->Text[this->content->SelectionStart - 1] != '\n') {
			while (it < this->content->TextLength && this->content->Text[it] == '\t') {
				++nextTabs;
				++it;
			}
        }

        if (nextTabs > tabs) tabs = nextTabs;
        
        if (e->Alt) {
            String^ str = it == 0 ? "" : "\r\n";
            for (int i = 0; i < tabs; ++i) {
                str += "\t";
            }
            if (!it) {
                str += "\r\n";
            }
            this->content->Text = this->content->Text->Insert(max(0, it - 2), str);
            this->content->SelectionStart = it + tabs;
            this->content->SelectionLength = 0;
        }
        else {
            int at = this->content->SelectionStart + 2 + tabs;
            this->content->Text = this->content->Text->Remove(this->content->SelectionStart, this->content->SelectionLength);
            String^ str = "\r\n";
            for (int i = 0; i < tabs; ++i) {
                str += "\t";
            }
            this->content->Text = this->content->Text->Insert(this->content->SelectionStart, str);
            this->content->SelectionStart = at;
            this->content->SelectionLength = 0;
        }
    }
}

void Bridge::Slide::SaveUndo(void) {
    if (!this->prev->Count || this->prev->Peek() != this->content->Text) {
        this->prev->Push(this->content->Text);
        this->prev_loc->Push(this->content->SelectionStart);

        this->next->Clear();
        this->next_loc->Clear();
    }
}

void Bridge::Slide::Indent(int delta) {
    int it = this->content->SelectionStart + this->content->SelectionLength;
    while (it > 0 && this->content->Text[it - 1] != '\n') {
        it--;
    }

    String^ ret = this->content->Text;
    int lines = 0;
    for (;;) {
        lines++;
        if (delta < 0) {
            if (ret[it] == '\t') {
                ret = ret->Remove(it, 1);
            }
        }
        else {
            ret = ret->Insert(it, "\t");
        }

        if (it <= this->content->SelectionStart) {
			break;
        }

        it--;
        while (it > 0 && ret[it - 1] != '\n') {
            it--;
        }
    } 

    int at = this->content->SelectionStart;
    int length = this->content->SelectionLength;
    this->content->Text = ret;
    if (delta > 0) {
        if (it < at) {
            this->content->SelectionStart = at + 1;
			if (length > 0) {
				this->content->SelectionLength = length + delta * lines - 1;
			}
            else {
                this->content->SelectionLength = length;
            }
        }
        else if (length > 0) {
            this->content->SelectionStart = at;
            this->content->SelectionLength = length + delta * lines;
        }
        else {
            this->content->SelectionStart = at + 1;
            this->content->SelectionLength = length;
        }
    }
    else {
        this->content->SelectionStart = at;
        this->content->SelectionLength = max(0, length + delta * lines);
    }
}

void Bridge::Slide::PrevLine()
{
    int it = this->content->SelectionStart;
    while (it > 0 && this->content->Text[it - 1] != '\n') {
        it--;
    }
    it -= 3;
    if (it < 0) {
        it = 0;
    }
    SelectLine(it);
}

void Bridge::Slide::NextLine()
{
    int it = this->content->SelectionStart;
    while (it > 0 && this->content->Text[it - 1] != '\n') {
        it--;
    }
    int at = SelectLine(it);
    if (at <= this->content->SelectionStart) {
        while (it < this->content->Text->Length - 1 && this->content->Text[it + 1] != '\r') {
            it++;
        }
        it += 3;
        it = min(it, this->content->Text->Length);
        SelectLine(it);
    }
}

int Bridge::Slide::SelectLine(int cursor)
{
    int start = cursor;
    while (start > 0 && content->Text[start - 1] != '\n') {
        start--;
    }
    int end = cursor;
    while (end < content->Text->Length - 1 && content->Text[end + 1] != '\n') {
        end++;
    }

    for (int i = 0; i < functor_start->Count; ++i) {
        if (functor_start[i] > end) {
            break;
        }
        if (start == functor_start[i]) {
            start = functor_end[i];
            break;
        }
    }

    this->content->SelectionStart = start;
    this->content->SelectionLength = max(0, end - start);

    return start;
}

void Bridge::Slide::Text_onChanged(System::Object^ sender, EventArgs^ args) {
    char* str = cstring_for(this->content->Text);
    if (strcmp(str, cache->buffer)) {
        slide_write_data(cache, str, strlen(str));
    }
    else {
        free(str);
    }

    this->content->ClearUndo();

    AdjustRect();
}

void Bridge::Slide::Text_resize(System::Object^ sender, EventArgs^ args) {
    AdjustRect();
}

/* workaround for weird autoscrolling bug? */
void Bridge::Slide::Text_Clicked(System::Object^ sender, MouseEventArgs^ args) {
}

void Bridge::Slide::AdjustRect() {
    /* sizing */
    System::Drawing::Size size = TextRenderer::MeasureText(this->content->Text, this->content->Font);

    int targ = max(50, size.Height + 30);
    if (targ != content->Height) {
        content->Height = targ;
    }
}

void Bridge::Slide::SetTabWidth(TextBox^ textbox, int tabWidth)
{
	HWND hwnd = static_cast<HWND>(textbox->Handle.ToPointer());

	array<int>^ tabStops = gcnew array<int>(1);
	tabStops[0] = tabWidth * 4;

	pin_ptr<int> pinnedTabStops = &tabStops[0];

	SendMessage(hwnd, EM_SETTABSTOPS, 1, reinterpret_cast<LPARAM>(pinnedTabStops));
}

static constexpr int slideStart = 100;
static constexpr int slideWidth = 100;
static constexpr int slideHeight = 80;
static constexpr int slideBorder = 2;
static constexpr int slideMargin = 5;
static constexpr int secondWidth = 20;
static constexpr int secondHeight = 4;

public ref class TimelineSlide: public Forms::Panel {
    String^ title{ nullptr };

    Label^ label;
    Panel^ thumbnail;
    Panel^ seconds;

public:
    double time{ 0 };
    bool addedHandler{ false };

    TimelineSlide() {
        label = gcnew Label();
        label->Size = Drawing::Size(slideWidth - 2 * slideBorder, slideHeight - 2 * slideBorder);
        label->Location = Drawing::Point(slideBorder, slideBorder);
        label->TextAlign = ContentAlignment::TopCenter;
        label->ForeColor = Color::White;

        thumbnail = gcnew Panel();
        thumbnail->Size = Drawing::Size(slideWidth, slideHeight);

        seconds = gcnew Panel();
        seconds->Size = Drawing::Size(0, secondHeight);
        seconds->Location = Drawing::Point(slideWidth, slideHeight / 2 - secondHeight / 2);
        seconds->BackColor = Color::White;

        this->Controls->Add(label);
        this->Controls->Add(thumbnail);
        this->Controls->Add(seconds);

        this->AutoSize = true;
        this->AutoSizeMode = Forms::AutoSizeMode::GrowAndShrink;
    }

    void Update(int num, char const* new_title, double new_time, bool trailing_valid) {
        if (!::Bridge::streq(new_title, title)) {
            this->title = gcnew String("Slide ") + num;
            this->label->Text = title;
        }

        if (trailing_valid) {
            thumbnail->BackColor = Color::Yellow;
        }
        else {
            thumbnail->BackColor = Color::LightGray;
        }

        if (new_time != time) {
            // update seconds
            this->seconds->Size = Drawing::Size((int) (new_time * secondWidth), secondHeight);
            this->time = new_time;
        }

        this->Invalidate();
    }
};

Bridge::Timeline::Timeline(struct timeline *timeline, Forms::Panel^ container) {
    this->timeline = timeline;

    toolbar = gcnew Panel();
    toolbar->BackColor = Color::FromArgb(255, 12, 12, 12);
    toolbar->Dock = Forms::DockStyle::Top;
    toolbar->Size = Drawing::Size(0, 35);

    timestamp = gcnew Label();
    timestamp->Anchor = Forms::AnchorStyles::None;
    timestamp->Size = Drawing::Size(110, 30);
    timestamp->Location = Drawing::Point(15, 6);
    timestamp->Font = gcnew Drawing::Font("Consolas", 11);
    timestamp->ForeColor = Color::White;
    timestamp->Text = "0:000";

    superFastBackward = gcnew Button();
    superFastBackward->FlatStyle = FlatStyle::Flat; 
    superFastBackward->FlatAppearance->BorderSize = 0; 
    superFastBackward->FlatAppearance->MouseDownBackColor = Color::Transparent; 
    superFastBackward->FlatAppearance->MouseOverBackColor = Color::Transparent;
    superFastBackward->Anchor = Forms::AnchorStyles::None;
    superFastBackward->Size = Drawing::Size(25, 25);
    superFastBackward->Location = Drawing::Point(-50, 4);
    superFastBackward->Image = Drawing::Image::FromFile("res\\scene_start.png");
    superFastBackward->BackgroundImageLayout = ImageLayout::Stretch;
    superFastBackward->Click += gcnew System::EventHandler(this, &Timeline::SuperBackward_onClick);
    superFastBackward->BackColor = Color::Transparent;
    
    fastBackward = gcnew Button();
    fastBackward->FlatStyle = FlatStyle::Flat; 
    fastBackward->FlatAppearance->BorderSize = 0; 
    fastBackward->FlatAppearance->MouseDownBackColor = Color::Transparent; 
    fastBackward->FlatAppearance->MouseOverBackColor = Color::Transparent;
    fastBackward->Anchor = Forms::AnchorStyles::None;
    fastBackward->Size = Drawing::Size(25, 25);
    fastBackward->Location = Drawing::Point(-31, 4);
    fastBackward->Image = Drawing::Image::FromFile("res\\prev_slide.png");
    fastBackward->BackgroundImageLayout = ImageLayout::Stretch;
    fastBackward->Click += gcnew System::EventHandler(this, &Timeline::FastBackward_onClick);
    fastBackward->BackColor = Color::Transparent;
    
    playButton = gcnew Button();
    playButton->FlatStyle = FlatStyle::Flat; 
    playButton->FlatAppearance->BorderSize = 0; 
    playButton->FlatAppearance->MouseDownBackColor = Color::Transparent; 
    playButton->FlatAppearance->MouseOverBackColor = Color::Transparent;
    playButton->Anchor = Forms::AnchorStyles::None;
    playButton->Size = Drawing::Size(25, 25);
    playButton->Location = Drawing::Point(-10, 4);
    playButton->Image = Drawing::Image::FromFile("res\\pause.png");
    playButton->BackgroundImageLayout = ImageLayout::Stretch;
    playButton->Click += gcnew System::EventHandler(this, &Timeline::Play_onClick);
    playButton->BackColor = Color::Transparent;
    
    exportButton = gcnew Button();
    exportButton->Text = "Export";
    exportButton->AutoSize = true;
    exportButton->Anchor = Forms::AnchorStyles::Right | Forms::AnchorStyles::Top;
    exportButton->Location = Drawing::Point(-85, 0);
    exportButton->BackColor = SystemColors::Control;
    exportButton->Click += gcnew System::EventHandler(this, &Timeline::Export_onClick);

    toolbar->Controls->Add(superFastBackward);
    toolbar->Controls->Add(fastBackward);
    toolbar->Controls->Add(playButton);
    toolbar->Controls->Add(timestamp);
    toolbar->Controls->Add(exportButton);
    
    Controls->Add(toolbar);

    mainTimeline = gcnew TimelinePanel();
    mainTimeline->Dock = Forms::DockStyle::Fill;
    mainTimeline->AutoScroll = true;
    mainTimeline->HorizontalScroll->Enabled = true;
    mainTimeline->HorizontalScroll->Visible = false;
    mainTimeline->Margin = Forms::Padding(0, toolbar->Height, 0, 0);

    slides = gcnew Panel();
    slides->Location = Point(0, 30);
    slides->Anchor = Forms::AnchorStyles::Left;
    slides->Padding = Forms::Padding(slideStart, 0, slideStart, 0);
	slides->AutoSize = true;
	slides->AutoSizeMode = Forms::AutoSizeMode::GrowAndShrink;

    cursor = gcnew Panel();
    cursor->Size = Drawing::Size(4, mainTimeline->Height);
    cursor->Margin = Forms::Padding();
    cursor->Anchor = Forms::AnchorStyles::Top | Forms::AnchorStyles::Bottom | Forms::AnchorStyles::Left;
    cursor->BackColor = Drawing::Color::White;

    cursorAnchor = gcnew Panel();
    cursorAnchor->BackColor = Color::Transparent;
    cursorAnchor->Dock = DockStyle::Left;
	cursorAnchor->AutoSize = true;
	cursorAnchor->AutoSizeMode = Forms::AutoSizeMode::GrowAndShrink;

    cursorAnchor->Controls->Add(cursor);
    cursorAnchor->Controls->Add(slides);
    
    mainTimeline->Controls->Add(cursorAnchor);
    set_click_handler(mainTimeline, gcnew System::EventHandler(this, &::Bridge::Timeline::Timeline_onClick));

    Controls->Add(mainTimeline);

    DoubleBuffered = true;
    Dock = Forms::DockStyle::Fill;

    container->Controls->Add(this);
}

void Bridge::Timeline::Play_onClick(System::Object^ sender, System::EventArgs^ args) {
    timeline_play_toggle(timeline);
}

void Bridge::Timeline::SuperBackward_onClick(System::Object^ sender, System::EventArgs^ args) {
    SceneStart();
}

void Bridge::Timeline::FastBackward_onClick(System::Object^ sender, System::EventArgs^ args) {
    PrevSlide();
}

void Bridge::Timeline::Export_onClick(System::Object^ sender, System::EventArgs^ args) {

    ExportForm^ f = gcnew ExportForm(this->timeline, this->timeline->handle->viewport);
    f->ShowDialog();
}

void Bridge::Timeline::Timeline_onClick(System::Object^ sender, EventArgs^ args) {
    MouseEventArgs^ e = dynamic_cast<MouseEventArgs^>(args);
    Control^ send = dynamic_cast<Control^>(sender);
    int true_x = slides->PointToClient(send->PointToScreen(e->Location)).X;
    size_t slide;
    double offset = 0;
    int pos = slideStart;
    for (slide = 0; slide < slides->Controls->Count; ++slide) {
        TimelineSlide^ slideUI = dynamic_cast<TimelineSlide^>(slides->Controls[slide]);
        int end = pos + slideWidth + (int) (slideUI->time * secondWidth) + slideMargin;

        if (true_x <= end || slide + 1 == slides->Controls->Count) {
            offset = max(0, (double) (true_x - (pos + slideWidth)) / secondWidth);
            break;
        }

        pos = end;
    }

    this->FindForm()->ActiveControl = nullptr;
    
    timeline_seek_to(this->timeline, ::timestamp{slide + 1, offset}, 1);
}

void Bridge::Timeline::ToggleInClosingMode(void) {
    this->inClosingMode = !this->inClosingMode;
}

void Bridge::Timeline::Update(void) {
    if (inClosingMode) return;

    /* lock */
    timeline_read_lock(timeline);

    // cache 
    cacheSlideNum = timeline->timestamp.slide;
    cacheOffset = timeline->timestamp.offset;
    cachedSlideCount = timeline->executor->slide_count;

    // play button + timestamp
    playButton->Image = Drawing::Image::FromFile(timeline->is_playing
		? "res\\pause.png"
		: "res\\play.png"
    );

    size_t const slide = timeline->seekstamp.slide;
    double const offset = timeline->seekstamp.offset;
    String^ time = System::String::Format("{0}:{1:0.000}", slide, offset);
    timestamp->Text = time;

    this->SuspendLayout();

    // actual slides
    // slide 0 is std lib
    // slide 1 is config 
    adjust_control_list<TimelineSlide>(slides->Controls, timeline->executor->slide_count - 2);
    int pos = slideStart;
    for (int i = 2; i < timeline->executor->slide_count; ++i) {
        TimelineSlide^ slideUI = dynamic_cast<TimelineSlide^>(slides->Controls[i - 2]);
        slideUI->Update(
            i - 1,
            timeline->executor->slides[i].title,
            timeline->executor->slides[i].seconds,
            timeline->executor->slides[i].trailing_valid
        );
        slideUI->Location = Drawing::Point(pos, 0);

        if (!slideUI->addedHandler) {
            slideUI->addedHandler = true;
            set_click_handler(slideUI, gcnew System::EventHandler(this, &::Bridge::Timeline::Timeline_onClick));
        }

        if (slide == (size_t) i - 1) {
            // update cursor
            cursor->Location = Point(pos + slideWidth + (int) (secondWidth * offset), 0);
        }

        pos += slideMargin + slideWidth + (int) (secondWidth * timeline->executor->slides[i].seconds);
    }

    this->ResumeLayout();

    /* release */
    timeline_read_unlock(timeline);
}

void Bridge::Timeline::SceneStart(void) {
    timeline_seek_to(timeline, ::timestamp{1, 0}, 1);
}

void Bridge::Timeline::SceneEnd(void) {
    timeline_seek_to(timeline, ::timestamp{cachedSlideCount, 0}, 1);
}

void Bridge::Timeline::PrevSlide(void) {
    if (this->cacheSlideNum != 1 && this->cacheOffset < DBL_EPSILON) {
		timeline_seek_to(timeline, ::timestamp{cacheSlideNum - 1, 0}, 1);
    }
    else {
		timeline_seek_to(timeline, ::timestamp{ cacheSlideNum, 0 }, 1);
    }
}

void Bridge::Timeline::NextSlide(void) {
    timeline_seek_to(timeline, ::timestamp{cacheSlideNum + 1, 0}, 1);
}

void Bridge::Timeline::TogglePlay(void) {
    timeline_play_toggle(timeline);
}


extern "C" {
#include "monocurl.h"
}


#include "mc_viewport.h"
#include "mc_renderer.h"
#include "mc_ui.h"
#include "mc_bridge.h"
#include "mc_scene_bridge.h"



using namespace Forms;
using namespace System::Drawing;

Bridge::Viewport::Viewport(struct viewport* viewport, Panel^ container) {
	this->viewport = viewport;

	this->Dock = Forms::DockStyle::Fill;
	container->Controls->Add(this);

    this->Resize += gcnew System::EventHandler(this, &Viewport::ScreenResize);
    this->SetStyle(
        ControlStyles::UserPaint |
        ControlStyles::AllPaintingInWmPaint |
        ControlStyles::OptimizedDoubleBuffer |
        ControlStyles::Opaque,
        true
    );
    this->buffer = gcnew Bitmap(this->Width, this->Height);
    this->DoubleBuffered = true;

    this->renderer = new MCRenderer();
    this->renderer->set_screen_size(this->Width, this->Height, 0);

    this->timestamp = gcnew Forms::Label();
    timestamp->BackColor = Color::Black;
    timestamp->ForeColor = Color::White;

}

//https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/d3d10-graphics-programming-guide-dxgi#handling-window-resizing
void Bridge::Viewport::ScreenResize(System::Object^ sender, System::EventArgs^ args)
{
    this->buffer = gcnew Bitmap(this->Width, this->Height);
    renderer->set_screen_size(Width, Height, 0);

    this->Invalidate();
}

void Bridge::Viewport::OnPaint(System::Windows::Forms::PaintEventArgs^ p) {
    this->renderer->render();

    // https://stackoverflow.com/questions/66565881/directx11-offscreen-rendering-output-image-is-flipepd
    System::Drawing::Rectangle rect(0, 0, buffer->Width, buffer->Height);
    auto data = this->buffer->LockBits(rect, System::Drawing::Imaging::ImageLockMode::WriteOnly, System::Drawing::Imaging::PixelFormat::Format32bppRgb);

    this->renderer->blit(reinterpret_cast<char*>(data->Scan0.ToPointer()));

	this->buffer->UnlockBits(data);

    p->Graphics->DrawImage(buffer, 0, 0);
}

void Bridge::Viewport::Update() {
    if (closing) {
        return;
    }

    this->Invalidate();

    this->renderer->recache(this->viewport);

    int slide = (int) viewport->handle->timeline->timestamp.slide;
    double offset = viewport->handle->timeline->timestamp.offset;
    this->timestamp->Text = System::String::Format("{0}:{1:0.000}", slide, offset);
}

void Bridge::Viewport::SetPresentation(bool presenting) {
    this->renderer->set_presentation_mode(presenting);
    this->presenting = presenting;

    if (presenting) {
        this->Controls->Add(timestamp);
    }
    else {
        this->Controls->Remove(timestamp);
    }
}

Bridge::Viewport::~Viewport() {
    delete renderer;
}

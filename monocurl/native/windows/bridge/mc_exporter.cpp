extern "C" {
#include "monocurl.h"
}
#include "mc_exporter.h"
#include "mc_renderer.h"
#include "mc_bridge.h"

using namespace System::Windows::Forms;
using namespace System;

Bridge::ExportForm::ExportForm(struct timeline* timeline, struct viewport* viewport) {
    ExportForm::instance = this;
    this->timeline = timeline;
    this->viewport = viewport;

    this->Size = System::Drawing::Size(350, 400);
    this->MaximumSize = this->Size;
    // this->ControlBox = false;
    this->Text = "Export Config";

    widthLabel = gcnew Label();
    widthLabel->Location = System::Drawing::Point(20, 30);
    widthLabel->Text = "Width:";
    this->Controls->Add(widthLabel);

    widthTextBox = gcnew TextBox();
    widthTextBox->Location = System::Drawing::Point(120, 30);
    widthTextBox->Size = System::Drawing::Size(100, 20);
    widthTextBox->Text = "1960";
    this->Controls->Add(widthTextBox);

    heightLabel = gcnew Label();
    heightLabel->Location = System::Drawing::Point(20, 60);
    heightLabel->Text = "Height:";
    this->Controls->Add(heightLabel);

    heightTextBox = gcnew TextBox();
    heightTextBox->Location = System::Drawing::Point(120, 60);
    heightTextBox->Size = System::Drawing::Size(100, 20);
    heightTextBox->Text = "1080";
    this->Controls->Add(heightTextBox);

    fpsLabel = gcnew Label();
    fpsLabel->Location = System::Drawing::Point(20, 90);
    fpsLabel->Text = "FPS:";
    this->Controls->Add(fpsLabel);

    fpsTextBox = gcnew TextBox();
    fpsTextBox->Location = System::Drawing::Point(120, 90);
    fpsTextBox->Size = System::Drawing::Size(100, 20);
    fpsTextBox->Text = "60";
    this->Controls->Add(fpsTextBox);

    exportLocationLabel = gcnew Label();
    exportLocationLabel->Location = System::Drawing::Point(20, 120);
    exportLocationLabel->Text = "Location:";
    this->Controls->Add(exportLocationLabel);

    exportLocation = gcnew Label();
    exportLocation->Location = System::Drawing::Point(120, 120);
    exportLocation->Size = System::Drawing::Size(100, 20);
    exportLocation->Text = "none";
    this->Controls->Add(exportLocation);

    locationButton = gcnew Button();
    locationButton->Location = System::Drawing::Point(220, 120);
    locationButton->Size = Drawing::Size(100, 30);
    locationButton->Text = "Pick";
    locationButton->Click += gcnew EventHandler(this, &ExportForm::PickButton_Click);
    this->Controls->Add(locationButton);

    exportButton = gcnew Button();
    exportButton->Location = System::Drawing::Point(70, 200);
    exportButton->Size = System::Drawing::Size(100, 30);
    exportButton->Text = "Export";
    exportButton->Click += gcnew EventHandler(this, &ExportForm::ExportButton_Click);
    this->Controls->Add(exportButton);

    cancelButton = gcnew Button();
    cancelButton->Location = System::Drawing::Point(180, 200);
    cancelButton->Size = System::Drawing::Size(100, 30);
    cancelButton->Text = "Cancel";
    cancelButton->Click += gcnew EventHandler(this, &ExportForm::CancelButton_Click);
    this->Controls->Add(cancelButton);

    error = gcnew Label();
    error->ForeColor = Drawing::Color::Red;
    error->Size = Drawing::Size(350, 90);
    error->Location = System::Drawing::Point(0, 230);
    error->TextAlign = Drawing::ContentAlignment::MiddleCenter;
    this->Controls->Add(error);

    progress = gcnew ProgressBar();
    progress->Location = Drawing::Point(25, 300);
    progress->Size = Drawing::Size(300, 30);
    this->Controls->Add(progress);

    this->CenterToScreen();
}

void Bridge::ExportForm::ExportButton_Click(System::Object^ sender, System::EventArgs^ e)
{
	uint32_t width, height, fps;
	if (!UInt32::TryParse(widthTextBox->Text, width) ||
		!UInt32::TryParse(heightTextBox->Text, height) ||
		!UInt32::TryParse(fpsTextBox->Text, fps))
	{
		this->error->Text = "Invalid input. Please enter integers for width, height, and FPS.";
		return;
	}

    if (width % 2 || height % 2 || width <= 0 || height <= 0) {
		this->error->Text = "Invalid input. Expected even positive width and height";
		return;
    }
    if (fps == 0) {
        this->error->Text = "Invalid Input. Expeceted positive fps";
        return;
    }
    if (path == nullptr) {
        this->error->Text = "Invalid Input. Please select a destination.";
        return;
    }

    this->error->Text = "";
    
    int upf = 1;

    char const* c = cstring_for(this->path);
    timeline_start_export(timeline, c, width, height, fps, upf);

    if (this->renderer) {
        delete renderer;
    }
    this->renderer = new MCRenderer();
    this->renderer->set_screen_size(width, height, true);
    if (this->data) {
        delete this->data;
    }
    this->data = new char[width * height * 4];
}

void Bridge::ExportForm::CancelButton_Click(System::Object^ sender, System::EventArgs^ e) {
    if (this->renderer) {
        timeline_interrupt_export(timeline);
        cancelled = true;
    }
    else {
        ExportForm::instance = nullptr;
        Hide();
        delete this->renderer;
        delete this->data;
        this->renderer = nullptr;
        this->data = nullptr;
    }
}

void Bridge::ExportForm::frame() {
    renderer->recache(viewport);
    renderer->render();
    renderer->blit(this->data);
    timeline_write_frame(timeline, (uint8_t*) this->data);
    this->progress->Value = this->progress->Maximum * timeline->timestamp.slide / timeline->handle->model->slide_count;
}

void Bridge::ExportForm::finish(System::String^ str) {
    if (!str) {
        if (System::IO::File::Exists(path)) {
            System::String^ args = System::String::Format("/e, /select, \"{0}\"", path);

            System::Diagnostics::ProcessStartInfo^ info = gcnew System::Diagnostics::ProcessStartInfo();
            info->FileName = "explorer";
            info->Arguments = args;
            System::Diagnostics::Process::Start("explorer.exe", path);
        }
		ExportForm::instance = nullptr;
        this->Hide();

        delete this->renderer;
        delete this->data;
        this->renderer = nullptr;
        this->data = nullptr;
    }
    else if (cancelled) {
        /* not great workaround, fix at some point */
		ExportForm::instance = nullptr;
        this->Hide();

        delete this->renderer;
        delete this->data;
        this->renderer = nullptr;
        this->data = nullptr;
    }
    else {
        this->error->Text = str;
    }
}

void Bridge::ExportForm::PickButton_Click(System::Object^ sender, System::EventArgs^ e) {
	SaveFileDialog^ saveFileDialog = gcnew SaveFileDialog();
	saveFileDialog->Title = "Select Save Location";
	saveFileDialog->Filter = "Video Files |*.mp4;";

	if (saveFileDialog->ShowDialog() == System::Windows::Forms::DialogResult::OK)
	{
        this->path = saveFileDialog->FileName;
        exportLocation->Text = System::IO::Path::GetFileName(path);
	}
}

extern "C" {
	void export_frame(timeline const *timeline) {
        Bridge::ExportForm^ f = Bridge::ExportForm::instance;
        if (f != nullptr) {
			Action^ updateAction = gcnew Action(f, &Bridge::ExportForm::frame);
            f->BeginInvoke(updateAction);
        }
	}

	void export_finish(timeline const *timeline, char const *error) {
        Bridge::ExportForm^ f = Bridge::ExportForm::instance;
        if (f != nullptr) {
			Action<System::String^>^ updateAction = gcnew Action<System::String^>(f, &Bridge::ExportForm::finish);
            System::String^ err = nullptr;
            if (error) {
                err = gcnew System::String(error);
            }
            f->BeginInvoke(updateAction, err);
        }
	}
}

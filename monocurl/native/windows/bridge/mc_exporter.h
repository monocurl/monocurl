#pragma once


struct viewport;
struct timeline;
class MCRenderer;

namespace Bridge {
	public ref class ExportForm : public System::Windows::Forms::Form
	{
		System::Windows::Forms::Label^ widthLabel;
		System::Windows::Forms::Label^ heightLabel;
		System::Windows::Forms::Label^ fpsLabel;
		System::Windows::Forms::Label^ exportLocationLabel;
		System::Windows::Forms::TextBox^ widthTextBox;
		System::Windows::Forms::TextBox^ heightTextBox;
		System::Windows::Forms::TextBox^ fpsTextBox;
		System::Windows::Forms::Label^ exportLocation;
		System::Windows::Forms::Button^ locationButton;
		System::Windows::Forms::Button^ exportButton;
		System::Windows::Forms::Button^ cancelButton;
		System::Windows::Forms::Label^ error;
		System::Windows::Forms::ProgressBar^ progress;
		viewport* viewport;
		timeline* timeline;
		bool cancelled{ false };

		System::String^ path;

		void ExportButton_Click(System::Object^ sender, System::EventArgs^ e);
		void CancelButton_Click(System::Object^ sender, System::EventArgs^ e);
		void PickButton_Click(System::Object^ sender, System::EventArgs^ e);

		char* data;
		MCRenderer* renderer;

	public:
		static ExportForm^ instance;
		ExportForm(struct timeline *timeline, struct viewport *viewport);

		void frame(void);
		void finish(System::String^ string);
	};
};

struct timeline;

extern "C" {
	void export_frame(timeline const *timeline);
	void export_finish(timeline const *timeline, char const *error);
}

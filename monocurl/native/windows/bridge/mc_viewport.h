struct viewport;
class MCRenderer;

namespace Bridge {

	public ref class Viewport : public System::Windows::Forms::Control {
		viewport* viewport;
		MCRenderer* renderer;

		bool presenting{ false };
		bool closing{ false };
		System::Windows::Forms::Label^ timestamp;
		System::Drawing::Bitmap^ buffer;
		
		void ScreenResize(System::Object^ sender, System::EventArgs^ args);

	protected:
		virtual void OnPaint(System::Windows::Forms::PaintEventArgs^ p) override;	

	public:
		Viewport(struct viewport* viewport, System::Windows::Forms::Panel^ container);

		void Update(void);
		void SetPresentation(bool presentation);
		void ToggleClosingMode(void) {
			closing = !closing;
		}

		~Viewport();
	};
}
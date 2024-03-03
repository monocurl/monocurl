#pragma once

extern "C" {
#include "mc_types.h"
}
#include "mc_bridge.h"
#include "mc_ui.h"

struct scene_handle;
struct raw_entry_model;
struct raw_group_model;
struct raw_slide_model;
struct raw_media_model;
struct raw_scene_model;

struct timeline;

namespace Bridge {
	template <typename T>
	bool adjust_control_list(System::Windows::Forms::Control::ControlCollection^ collection, size_t new_size) {
		bool ret = false;
		
		while (collection->Count > new_size) {
			collection->RemoveAt(collection->Count - 1);
			ret = true;
		}

		while (collection->Count < new_size) {
			T^ const elem = gcnew T();
			collection->Add(elem);
			ret = true;
		}

		return ret;
	}

	public ref class SceneBridge {
		static SceneBridge^ active_list = nullptr;

		// opaque pointer
		scene_handle *handle;

		System::Windows::Forms::Control^ root;
		System::Windows::Forms::Control^ rootSplit;

		// handle for editor
		System::Windows::Forms::Panel^ editor;

		// handle for media list
		System::Windows::Forms::Panel^ media;

		// handle for timeline
		Timeline^ timeline;

		System::Windows::Forms::Panel^ viewportContainer;
		Viewport^ viewport;
	public:

		SceneBridge(
			System::String^ path,
			System::Windows::Forms::Control^ root,
			System::Windows::Forms::Control^ rootSplit,
			System::Windows::Forms::Panel^ editor,
			System::Windows::Forms::Panel^ media,
			System::Windows::Forms::Panel^ timeline,
			System::Windows::Forms::Panel^ viewportContainer
		);

		void reroot_timeline(void);
		void reroot_viewport(void);

		void reroot_scene(void);
		void reroot_scene(System::IntPtr ptr, bool force);
		void reroot_media(System::IntPtr ptr, Media^ ui, bool force);
		void reroot_slide(System::IntPtr ptr, Slide^ ui, bool force);

		void insert_media_image(System::String^ name);
		void force_save(void);

		void toggle_play(void);
		void toggle_presentation(void);

		void prev_slide(void);
		void next_slide(void);
		void scene_start(void);
		void scene_end(void);

		void add_slide(void);

		~SceneBridge();
		
		static void init_default_scene(System::String^ path);

		static SceneBridge^ find_scene(::timeline const* model);
		static SceneBridge^ find_scene(::viewport const* model);

		static SceneBridge^ find_scene(raw_scene_model const* model);
		static Slide^		find_slide(raw_slide_model const* model);
	};
}

extern "C" {
	void slide_flush(raw_slide_model *ptr, mc_bool_t is_global);
	void scene_flush(raw_scene_model *ptr, mc_bool_t is_global);
	
	void timeline_flush(timeline *timeline);
	void viewport_flush(viewport *timeline);
}

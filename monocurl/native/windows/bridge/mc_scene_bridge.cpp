extern "C" {
#include "monocurl.h"
}

#include "mc_viewport.h"
#include "mc_ui.h"
#include "mc_scene_bridge.h"

using namespace System::Windows::Forms;

void Bridge::SceneBridge::init_default_scene(System::String^ path) {
	char const *const path_null_terminated = cstring_for(path);
	mc_status_t const ret = file_write_default_scene(path_null_terminated);
	free((char *) path_null_terminated);

	if (ret != MC_STATUS_SUCCESS) throw ret;
}

Bridge::SceneBridge::SceneBridge(
	System::String^ path,
	Control^ root,
	Control^ rootSplit,
	Panel^ editor,
	Panel^ media,
	Panel^ timeline,
	Panel^ viewportContainer
) {
	char const *const path_null_terminated = cstring_for(path);
	scene_handle *const handle = file_read_sync(path_null_terminated);
	/* handle owns the path and is reponsible for freeing */

	if (!handle) throw MC_STATUS_FAIL;

	this->handle = handle;
	
	this->root = root;
	this->rootSplit = rootSplit;
	this->editor = editor;
	this->media = media;
	this->timeline = gcnew Timeline(this->handle->timeline, timeline);
	this->viewportContainer = viewportContainer;
	this->viewport = gcnew Viewport(this->handle->viewport, this->viewportContainer);

	/* add to global instance tracking list */
	SceneBridge::active_list = this;

	this->reroot_scene();

	/* initialize at zero */
    struct timestamp const t = { 1, 0 };
    timeline_seek_to(this->handle->timeline, t, 1);
}

void Bridge::SceneBridge::insert_media_image(System::String^ name) {
	const char* cname = cstring_for(System::IO::Path::GetFileNameWithoutExtension(name));
	const char* cpath = cstring_for(name);

	media_insert_image(this->handle->model, cname, cpath);
}

void Bridge::SceneBridge::force_save(void) {
	file_write_model(this->handle);
}

void Bridge::SceneBridge::prev_slide(void) {
	this->timeline->PrevSlide();
}

void Bridge::SceneBridge::next_slide(void) {
	this->timeline->NextSlide();
}

void Bridge::SceneBridge::scene_start(void) {
	this->timeline->SceneStart();
}

void Bridge::SceneBridge::scene_end(void) {
	this->timeline->SceneEnd();
}

void Bridge::SceneBridge::toggle_play(void) {
	this->timeline->TogglePlay();
}

void Bridge::SceneBridge::toggle_presentation(void) {
	timeline_toggle_presentation_mode(handle->timeline);

	if (this->viewport->FindForm()->ActiveControl != nullptr) {
		this->viewport->FindForm()->ActiveControl = nullptr;
	}

	viewport->SetPresentation(handle->timeline->in_presentation_mode != 0);

	if (handle->timeline->in_presentation_mode) {
		this->root->Controls->Remove(this->rootSplit);

		this->viewportContainer->Controls->Remove(this->viewport);
		this->root->Controls->Add(this->viewport);
	}
	else {
		this->root->Controls->Remove(this->viewport);
		this->viewportContainer->Controls->Add(this->viewport);

		this->root->Controls->Add(this->rootSplit);
	}
}

void Bridge::SceneBridge::add_slide(void) {
	struct raw_scene_model const* const scene = this->handle->model;
	insert_slide_after(scene->slides[scene->slide_count - 1]);
}

static void up_scene(raw_scene_model* scene) {
	if (!scene->dirty) return;
	scene->dirty = false;
}

static void up_slide(raw_slide_model* slide) {
	if (!slide->dirty) return;

	slide->dirty = false;
	up_scene(slide->scene);
}

void Bridge::SceneBridge::reroot_timeline(void) {
	Action^ updateAction = gcnew Action(this->timeline, &Timeline::Update);
	Application::OpenForms[0]->BeginInvoke(updateAction);
}

void Bridge::SceneBridge::reroot_viewport(void) {
	Action^ updateAction = gcnew Action(this->viewport, &Viewport::Update);
	Application::OpenForms[0]->BeginInvoke(updateAction);
}

void Bridge::SceneBridge::reroot_slide(IntPtr ptr, Slide^ ui, bool force) {
	raw_slide_model* slide = (raw_slide_model*) ptr.ToPointer();
	slide->dirty = false;

	ui->SuspendLayout();

	ui->Update(slide);

	ui->ResumeLayout();

	up_slide(slide);
}

void Bridge::SceneBridge::reroot_media(IntPtr ptr, Media^ ui, bool force) {
	raw_media_model* media = (raw_media_model*) ptr.ToPointer();
	media->dirty = false;

	// adjust name, and adjust link if necessary
	ui->SetName(media->name);
	ui->SetPath(media->path);
	ui->SetMedia(media);
}

void Bridge::SceneBridge::reroot_scene(IntPtr ptr, bool force) {
	raw_scene_model* scene = (raw_scene_model*) ptr.ToPointer();
	/* no need for dirty checks, always true */

	this->media->SuspendLayout();
	bool media_force = adjust_control_list<Media>(this->media->Controls, scene->media_count) || force;
	for (mc_ind_t i = 0; i < scene->media_count; ++i) {
		this->reroot_media(IntPtr(scene->media[scene->media_count - 1 - i]), dynamic_cast<Media^>(this->media->Controls[i]), media_force);
		this->media->Controls[i]->TabIndex = (int) (scene->media_count - 1 - i);
	}
	this->media->ResumeLayout();
	
	this->editor->SuspendLayout();
	bool slide_force = adjust_control_list<Slide>(this->editor->Controls, scene->slide_count) || force;
	for (mc_ind_t i = 0; i < scene->slide_count; ++i) {
		this->reroot_slide(IntPtr(scene->slides[scene->slide_count - 1 - i]), dynamic_cast<Slide^>(this->editor->Controls[i]), slide_force);
		this->editor->Controls[i]->TabIndex = (int) (scene->slide_count - 1 - i);
	}	
	this->editor->ResumeLayout();

	up_scene(scene);
}

void Bridge::SceneBridge::reroot_scene(void) {
	this->reroot_scene(IntPtr(this->handle->model), false);
}

Bridge::SceneBridge::~SceneBridge() {
	this->timeline->ToggleInClosingMode();
	this->viewport->ToggleClosingMode();

	scene_handle_free(this->handle);

	/* remove from global instance tracking list */
	SceneBridge::active_list = nullptr;
}

Bridge::SceneBridge^ Bridge::SceneBridge::find_scene(::timeline const* model) {
	return SceneBridge::active_list;
}

Bridge::SceneBridge^ Bridge::SceneBridge::find_scene(raw_scene_model const* model) {
	return SceneBridge::active_list;
}

Bridge::SceneBridge^ Bridge::SceneBridge::find_scene(::viewport const* model) {
	return SceneBridge::active_list;
}

Bridge::Slide^ Bridge::SceneBridge::find_slide(raw_slide_model const* model) {
	SceneBridge^ const scene = find_scene(model->scene);

	size_t const i = slide_index_in_parent(model);

	return dynamic_cast<Slide^>(scene->editor->Controls[scene->editor->Controls->Count - 1 - i]);
}

void slide_flush(raw_slide_model* ptr, mc_bool_t is_global) {
	auto const scene = Bridge::SceneBridge::find_scene(ptr->scene);
	auto const slide = Bridge::SceneBridge::find_slide(ptr);

	if (System::Windows::Forms::Application::OpenForms[0]->InvokeRequired) {
		Action<IntPtr, Bridge::Slide^, bool>^ a = gcnew Action<IntPtr, Bridge::Slide^, bool>(scene, &Bridge::SceneBridge::reroot_slide);
		System::Windows::Forms::Application::OpenForms[0]->BeginInvoke(a, IntPtr(ptr), slide, false);
	}
	else {
		scene->reroot_slide(IntPtr(ptr), slide, false);
	}
}

void scene_flush(raw_scene_model* ptr, mc_bool_t is_global) {
	auto const scene = Bridge::SceneBridge::find_scene(ptr);

	if (System::Windows::Forms::Application::OpenForms[0]->InvokeRequired) {
		Action^ a = gcnew Action(scene, &Bridge::SceneBridge::reroot_scene);
		System::Windows::Forms::Application::OpenForms[0]->BeginInvoke(a);
	}
	else {
		scene->reroot_scene();
	}
}

void timeline_flush(timeline* timeline) {
	Bridge::SceneBridge::find_scene(timeline)->reroot_timeline();
}

void viewport_flush(viewport* viewport) {
	Bridge::SceneBridge::find_scene(viewport)->reroot_viewport();
}


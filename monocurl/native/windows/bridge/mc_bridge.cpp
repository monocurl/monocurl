extern "C" {
#include "monocurl.h"
}
#include "mc_bridge.h"

void Bridge::Bridge::init(void) {
	monocurl_init();
}

void Bridge::Bridge::free(void) {
	monocurl_free();
}


// allocates if buffer is NULL
char *cstring_for(System::String^ net_string, char buffer[]) {
	if (net_string->Length == 0) return (char*) calloc(1, sizeof(char));

	array<System::Byte>^ bytes = System::Text::Encoding::UTF8->GetBytes(net_string);
	
	/* we use malloc since *may* need to be sent to c code */

	if (!buffer) buffer = (char*) malloc(bytes->Length + 1);
	pin_ptr<System::Byte> const byte_ptr = &bytes[0];
	memcpy(buffer, byte_ptr, bytes->Length);

	int length = 0;
	for (int i = 0; i < bytes->Length; ++i) {
		if (byte_ptr[i] != '\r') {
			buffer[length++] = byte_ptr[i];
		}
	}

	buffer[length] = '\0';

	return buffer;
}

static void get_file(System::String^ name, char buffer[]) {
	GetModuleFileNameA(nullptr, buffer, MAX_PATH);
	System::String^ executable_path = gcnew System::String(buffer);
	System::String^ root = System::IO::Path::GetDirectoryName(executable_path);
	System::String^ default_scene = System::IO::Path::Join(root, name);
	::cstring_for(default_scene, buffer);
}

int Bridge::streq(char const* c_string, System::String^ net_string) {
	if (!net_string || !c_string) return 0; // typically means we want to refresh it

	int i = 0;
	char const* x;
	for (x = c_string; *x && i < net_string->Length; ++i, ++x) {
		if (*x != net_string[i]) {
			return 0;
		}
	}

	return !*x && i == net_string->Length;
}

char *Bridge::cstring_for(System::String^ net_string) {
	return ::cstring_for(net_string, NULL);
}

extern "C" {
	/* not terribly inefficient, but it's debug only so it should be fine */

	void debug_write_log(char const *msg) {
		System::String^ net_str = gcnew System::String(msg);
		System::Diagnostics::Debug::Write(net_str);
	}

	char const *default_scene_path(void) {
		static int lazy_initialized = 0;
		static char buffer[MAX_PATH + 1];
	
		if (!lazy_initialized) {
			get_file("mc_default_scene.mcf", buffer);
			lazy_initialized = 1;
		}

		return buffer;
	}
	
	char const *std_lib_path(void) {
		static int lazy_initialized = 0;
		static char buffer[MAX_PATH + 1];
	
		if (!lazy_initialized) {
			get_file("libmc.mcf", buffer);
			lazy_initialized = 1;
		}

		return buffer;
	}

	char const* tex_binary_path(void) {
		static int lazy_initialized = 0;
		static char buffer[MAX_PATH + 1];

		if (!lazy_initialized) {
			get_file("TinyTex\\bin\\windows\\", buffer);
			lazy_initialized = 1;
		}

		return buffer;
	}

	char const* tex_intermediate_path(void) {
		static int lazy_initialized = 0;
		static char buffer[MAX_PATH + 1];

		if (!lazy_initialized) {
			GetTempPathA(sizeof buffer, buffer);
			System::String^ general_temp = gcnew System::String(buffer);
			System::String^ full = System::IO::Path::Combine(general_temp, "monocurl\\tex\\");

			if (!System::IO::Directory::Exists(full)) {
				try {
					System::IO::Directory::CreateDirectory(full);
				}
				catch (System::Exception^ ex) {

				}
			}

			cstring_for(full, buffer);
			lazy_initialized = 1;
		}

		return buffer;
	}

	char const *path_translation(char const *path) {
		return _strdup(path);
	}
}


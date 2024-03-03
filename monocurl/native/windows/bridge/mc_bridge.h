#pragma once

namespace Bridge {

public ref class Bridge {
public:
	static void init(void);
	static void free(void);
};

char *cstring_for(System::String^ net_string);
int streq(char const* cstring, System::String^ net_string);

}

/* c callback functions */
/* technically unnecessary to include in header, but for safe keeping */
extern "C" {
	void debug_write_log(char const *str);
	char const *default_scene_path(void);
	char const *std_lib_path(void);
	char const *tex_binary_path(void);
	char const *tex_intermediate_path(void);
	char const *path_translation(char const *path);
}

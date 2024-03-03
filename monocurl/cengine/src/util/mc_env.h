#pragma once
#include "mc_assert.h"

/* http://web.archive.org/web/20191012035921/http://nadeausoftware.com/articles/2012/01/c_c_tip_how_use_compiler_predefined_macros_detect_operating_system
 */

// not really os but more os/architecture combination
#define MC_ENV_OS_MACOS_INTEL 1
#define MC_ENV_OS_MACOS_ARM 2
#define MC_ENV_OS_IOS_DEVICE 4
#define MC_ENV_OS_IOS_SIMULATOR 8
#define MC_ENV_OS_LINUX 16
#define MC_ENV_OS_WINDOWS_32 32
#define MC_ENV_OS_WINDOWS_64 64

#define MC_ENV_OS_MACOS (MC_ENV_OS_MACOS_ARM | MC_ENV_OS_MACOS_INTEL)
#define MC_ENV_OS_IOS (MC_ENV_OS_IOS_DEVICE | MC_ENV_OS_IOS_SIMULATOR)
#define MC_ENV_OS_DARWIN (MC_ENV_OS_MACOS | MC_ENV_OS_IOS)
#define MC_ENV_OS_POSIX (MC_ENV_OS_DARWIN | MC_ENV_OS_LINUX)

#define MC_ENV_OS_WINDOWS (MC_ENV_OS_WINDOWS_32 | MC_ENV_OS_WINDOWS_64)

/* c compiler */
#define MC_ENV_CC_GCC 1
#define MC_ENV_CC_CLANG 2
#define MC_ENV_CC_GCCLIKE MC_ENV_CC_GCC | MC_ENV_CC_CLANG
#define MC_ENV_CC_MSVC 4

#if defined(_WIN64)
#define MC_ENV_OS MC_ENV_OS_WINDOWS_64
#elif defined(_WIN32)
#define MC_ENV_OS MC_ENV_OS_WINDOWS_32
#elif defined(__linux__)
#define MC_ENV_OS MC_ENV_OS_LINUX
#elif defined(__APPLE__) && defined(__MACH__)
#include <TargetConditionals.h>
#if TARGET_IPHONE_SIMULATOR == 1
#define MC_ENV_OS MC_ENV_OS_IOS_SIMULATOR
#elif TARGET_OS_IPHONE == 1
#define MC_ENV_OS MC_ENV_OS_IOS_DEVICE
#elif TARGET_OS_MAC == 1
#if defined(__arm__)
#define MC_ENV_OS MC_ENV_OS_MACOS_ARM
#else
#define MC_ENV_OS MC_ENV_OS_MACOS_INTEL
#endif
#endif
#else
#error Unsupported operating system or architecture
#endif

#if MC_ENV_OS & MC_ENV_OS_WINDOWS
#define MC_ENV_OS_ENDLINE "\r\n"
#else
#define MC_ENV_OS_ENDLINE "\n"
#endif

#if defined(__clang__)
#define MC_ENV_CC MC_ENV_CC_CLANG
#elif defined(__GNUG__)
#define MC_ENV_CC MC_ENV_CC_GCC
#elif defined(__GNUC__)
#define MC_ENV_CC MC_ENV_CC_GCC
#elif defined(_MSC_VER)
#define MC_ENV_CC MC_ENV_CC_MSVC
#else
#error Unsupported compiler
#endif

#define MC_ENV_IN_CPP defined(__cplusplus)

#ifdef _MC_INTERNAL
#define MC_INTERNAL 1
#else
#define MC_INTERNAL 0
#endif

#ifdef NDEBUG
#define MC_LOGGING 0
#define MC_DEBUG 0
#else
#define MC_LOGGING 1
#define MC_DEBUG 1
#endif

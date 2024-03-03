//
//  main.c
//  Monocurl
//
//  Created by Manu Bhat on 9/10/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <locale.h>

#include "mc_memory.h"
#include "mc_stdlib.h"
#include "mc_threading.h"
#include "monocurl.h"

#define MC_LOG_TAG "monocurl"
#include "mc_log.h"

void
monocurl_init(void)
{
    mc_threading_init();

    libmc_stdlib_init();

    setlocale(LC_ALL, "en_US.UTF-8");

    mc_logn_static("init", "%s", "");
}

char const *
monocurl_version_str(void)
{
    return APP_VERSION;
}

void
monocurl_free(void)
{
    mc_threading_free();

    mc_logn_static("free", "%s", "");
}

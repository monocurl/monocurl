# sourced by the macos and linux smoke tests

# run every bundled default scene through all of its slides; this loads every
# linked library, finds the bundled assets and std, and runs the kernel jit
smoke_scenes() {
    local bin="$1" scenes="$2" scene
    local count=0
    for scene in "$scenes"/*.mcs; do
        echo "[smoke] stepping $(basename "$scene")"
        "$bin" video "$scene" --list-slides >/dev/null
        count=$((count + 1))
    done
    (( count > 0 )) || { echo "[error] no default scenes found in $scenes" >&2; return 1; }
}

# export one frame through the gpu renderer
smoke_image() {
    local bin="$1" scene="$2" out="$3"
    echo "[smoke] exporting a frame of $(basename "$scene")"
    "$bin" image "$scene" -r small -o "$out" >/dev/null
    [[ -s "$out" ]] || { echo "[error] image export produced no file" >&2; return 1; }
}

# launch the desktop app and require it to still be running after a while
smoke_gui() {
    local bin="$1" log="$2" secs="${SMOKE_GUI_SECONDS:-20}"
    echo "[smoke] launching the app for ${secs}s"
    "$bin" >"$log" 2>&1 &
    local pid=$!
    sleep "$secs"
    if ! kill -0 "$pid" 2>/dev/null; then
        local status=0
        wait "$pid" || status=$?
        echo "[error] the app exited with status $status during launch" >&2
        cat "$log" >&2
        return 1
    fi
    kill "$pid"
    wait "$pid" 2>/dev/null || true
    echo "[smoke] app stayed up"
}

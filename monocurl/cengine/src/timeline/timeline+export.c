//
//  timeline+export.c
//  monocurl
//
//  Created by Manu Bhat on 11/30/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

#include <stdint.h>

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/avutil.h>
#include <libswscale/swscale.h>

#include "callback.h"
#include "timeline+export.h"
#include "timeline+simulate.h"

struct encoder_context {
    AVCodecContext *c;
    AVPacket *out;
    AVFrame *frame;
    AVFormatContext *fc;
    AVStream *stream;

    struct SwsContext *sws;

    char const *file;
};

/* https://ffmpeg.org/doxygen/3.4/encode_video_8c-example.html#a8 */
static mc_status_t
write_frame(
    struct timeline *timeline, uint8_t *frame_buffer,
    struct encoder_context context
)
{
    int ret;

    if (context.frame && av_frame_make_writable(context.frame)) {
        export_finish(timeline, "Could not make a frame writable");
        return MC_STATUS_FAIL;
    }

    if (frame_buffer) {
        uint8_t const *frame[1] = { frame_buffer };
        int line_size[1] = { (int) (timeline->export_mode.w * sizeof(uint32_t)
        ) };
        ret = sws_scale(
            context.sws, frame, line_size, 0, (int) timeline->export_mode.h,
            context.frame->data, context.frame->linesize
        );

        if (ret < 0) {
            export_finish(timeline, "Error translating from BGRA to YUV");
            return MC_STATUS_FAIL;
        }
    }

    /* send the frame to the encoder */
    ret = avcodec_send_frame(context.c, context.frame);
    if (ret < 0) {
        export_finish(timeline, "Error sending a frame for encoding");
        return MC_STATUS_FAIL;
    }

    while (ret >= 0) {
        ret = avcodec_receive_packet(context.c, context.out);
        if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) {
            break;
        }
        else if (ret < 0) {
            export_finish(
                timeline, "Error during encoding while receving packets"
            );
            av_packet_unref(context.out);
            return MC_STATUS_FAIL;
        }

        av_packet_rescale_ts(
            context.out, context.c->time_base, context.stream->time_base
        );
        context.out->stream_index = context.stream->index;

        ret = av_interleaved_write_frame(context.fc, context.out);
        if (ret < 0) {
            export_finish(timeline, "Error writing frames");
            av_packet_unref(context.out);
            return MC_STATUS_FAIL;
        }
        av_packet_unref(context.out);
    }

    if (context.frame) {
        ++context.frame->pts;
    }

    return MC_STATUS_SUCCESS;
}

static mc_ternary_status_t
export_slide(
    struct timeline *timeline, mc_ind_t index, struct encoder_context context
)
{
    double dt;

    mc_rwlock_writer_lock(timeline->state_lock);

    dt = 1.0 / (timeline->export_mode.upf * timeline->export_mode.fps);
    if (timeline_slide_startup(timeline, index, 1) < 0) {
        export_finish(timeline, "Error in slide startup");
        mc_rwlock_writer_unlock(timeline->state_lock);
        return MC_TERNARY_STATUS_FAIL;
    }

    mc_rwlock_writer_unlock(timeline->state_lock);

    // initial setup
    for (;;) {
        mc_rwlock_writer_lock(timeline->state_lock);

        mc_ternary_status_t const ret =
            timeline_frame(timeline, dt, timeline->export_mode.upf);

        if (context.c) {
            export_frame(timeline);
            mc_rwlock_writer_unlock(timeline->state_lock);
            mc_cond_variable_wait(timeline->has_q, timeline->q_lock);
            mc_rwlock_writer_lock(timeline->state_lock);

            if (write_frame(
                    timeline, timeline->export_mode.frame_buffer, context
                )) {
                mc_rwlock_writer_unlock(timeline->state_lock);
                return MC_TERNARY_STATUS_FAIL;
            }
        }
        mc_rwlock_writer_unlock(timeline->state_lock);

        if (ret == MC_TERNARY_STATUS_FAIL) {
            mc_rwlock_reader_lock(timeline->state_lock);
            export_finish(
                timeline, "Operation cancelled by user or execution error"
            );
            mc_rwlock_reader_unlock(timeline->state_lock);
            return MC_TERNARY_STATUS_FAIL;
        }
        else if (ret == MC_TERNARY_STATUS_FINISH) {
            break;
        }
    }

    if (timeline->timestamp.slide < timeline->handle->model->slide_count - 1) {
        mc_rwlock_writer_lock(timeline->state_lock);
        ++timeline->timestamp.slide;
        timeline->timestamp.offset = 0;
        timeline_blit_trailing_cache(timeline);
        mc_rwlock_writer_unlock(timeline->state_lock);
    }
    else {
        return MC_TERNARY_STATUS_FINISH; // finished
    }

    return MC_TERNARY_STATUS_CONTINUE;
}

// called from timeline thread
// assumes no editing during the export (but cancels are allowed)
void
timeline_export(struct timeline *timeline)
{
    timeline->timestamp = (struct timestamp){ 1, 0 };

    struct raw_scene_model *scene = timeline->handle->model;
    struct timeline_export_mode const export = timeline->export_mode;

    mc_rwlock_writer_lock(timeline->state_lock);
    timeline->is_playing = 0;
    mc_rwlock_writer_unlock(timeline->state_lock);

    AVCodec const *codec = NULL;
    AVCodecContext *ctx = NULL;
    struct SwsContext *sws = NULL;
    AVPacket *packet_buffer = NULL;
    AVFrame *frame = NULL;
    AVFormatContext *fc = NULL;
    AVStream *stream = NULL;

    avformat_alloc_output_context2(&fc, NULL, NULL, export.out_path);
    if (!fc) {
        mc_rwlock_reader_lock(timeline->state_lock);
        export_finish(timeline, "Codec format not found!");
        mc_rwlock_reader_unlock(timeline->state_lock);
        goto deconstructor;
    }

    if (!(codec = avcodec_find_encoder(AV_CODEC_ID_H264))) {
        mc_rwlock_reader_lock(timeline->state_lock);
        export_finish(timeline, "Codec not found");
        mc_rwlock_reader_unlock(timeline->state_lock);
        goto deconstructor;
    }

    if (!(stream = avformat_new_stream(fc, NULL))) {
        mc_rwlock_reader_lock(timeline->state_lock);
        export_finish(timeline, "Format stream not found");
        mc_rwlock_reader_unlock(timeline->state_lock);
        goto deconstructor;
    }
    else {
        stream->time_base = (AVRational){ 1, (int) export.fps };
        stream->id = (int) fc->nb_streams - 1;
    }

    if (!(ctx = avcodec_alloc_context3(codec))) {
        mc_rwlock_reader_lock(timeline->state_lock);
        export_finish(timeline, "Could not allocate video encoder context\n");
        mc_rwlock_reader_unlock(timeline->state_lock);
        goto deconstructor;
    }
    else {
        ctx->width = (int) export.w;
        ctx->height = (int) export.h;
        ctx->time_base = (AVRational){ 1, (int) export.fps };
        ctx->framerate = (AVRational){ (int) export.fps, 1 };
        ctx->pix_fmt = AV_PIX_FMT_YUV420P;
        ctx->color_range = AVCOL_RANGE_MPEG;
        ctx->bit_rate = 1 << 30;
        ctx->rc_max_rate = 1 << 30;
        ctx->max_b_frames = 1;
        ctx->gop_size = 1;
    }

    if (!(sws = sws_getContext(
              (int) export.w, (int) export.h, AV_PIX_FMT_BGRA, (int) export.w,
              (int) export.h, AV_PIX_FMT_YUV420P, SWS_BICUBIC, NULL, NULL, NULL
          ))) {
        mc_rwlock_reader_lock(timeline->state_lock);
        export_finish(timeline, "Could not allocate sws context\n");
        mc_rwlock_reader_unlock(timeline->state_lock);
        goto deconstructor;
    }

    if (fc->oformat->flags & AVFMT_GLOBALHEADER) {
        ctx->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
    }

    AVDictionary *opt = NULL;
    av_dict_set(&opt, "preset", "slow", 0);
    av_dict_set(&opt, "crf", "10", 0);
    if (avcodec_open2(ctx, codec, &opt) < 0) {
        mc_rwlock_reader_lock(timeline->state_lock);
        export_finish(timeline, "Could not open codec [1]");
        mc_rwlock_reader_unlock(timeline->state_lock);
        goto deconstructor;
    }
    av_dict_free(&opt);

    if (avcodec_parameters_from_context(stream->codecpar, ctx) < 0) {
        mc_rwlock_reader_lock(timeline->state_lock);
        export_finish(timeline, "Could not open codec [2]");
        mc_rwlock_reader_unlock(timeline->state_lock);
        goto deconstructor;
    }

    /* YUV420 configuration from
     * https://ffmpeg.org/doxygen/trunk/encoding-example_8c-source.html
     */
    if (!(packet_buffer = av_packet_alloc())) {
        mc_rwlock_reader_lock(timeline->state_lock);
        export_finish(timeline, "Could not create a packet buffer");
        mc_rwlock_reader_unlock(timeline->state_lock);
        goto deconstructor;
    }

    // * 4 is arbitrary, just needs to be big enough to handle the frame.
    // However 32bpp means 4 should generally be enough
    if (av_new_packet(packet_buffer, (int) export.w * (int) export.h * 4) < 0) {
        mc_rwlock_reader_lock(timeline->state_lock);
        export_finish(timeline, "Could not create a frame buffer");
        mc_rwlock_reader_unlock(timeline->state_lock);
        goto deconstructor;
    }

    if (!(frame = av_frame_alloc())) {
        mc_rwlock_reader_lock(timeline->state_lock);
        export_finish(timeline, "Could not initialize a frame");
        mc_rwlock_reader_unlock(timeline->state_lock);
        goto deconstructor;
    }
    else {
        frame->width = (int) export.w;
        frame->height = (int) export.h;
        frame->color_range = AVCOL_RANGE_MPEG;
        frame->format = AV_PIX_FMT_YUV420P;
        frame->pts = 0;
    }

    if (av_frame_get_buffer(frame, 0) < 0) {
        mc_rwlock_reader_lock(timeline->state_lock);
        export_finish(timeline, "Could not create a frame buffer");
        mc_rwlock_reader_unlock(timeline->state_lock);

        goto deconstructor;
    }

    av_dump_format(fc, 0, export.out_path, 1);

    if (!(fc->flags & AVFMT_NOFILE) &&
        avio_open(&fc->pb, export.out_path, AVIO_FLAG_WRITE) < 0) {
        mc_rwlock_reader_lock(timeline->state_lock);
        export_finish(timeline, "Could not open file");
        mc_rwlock_reader_unlock(timeline->state_lock);
        goto deconstructor;
    }

    if (avformat_write_header(fc, NULL) < 0) {
        mc_rwlock_reader_lock(timeline->state_lock);
        export_finish(timeline, "Could not write file headers");
        mc_rwlock_reader_unlock(timeline->state_lock);
        goto deconstructor;
    }

    struct encoder_context encoder_context = {
        .c = ctx,
        .out = packet_buffer,
        .frame = frame,
        .fc = fc,
        .stream = stream,
        .sws = sws,
        .file = export.out_path,
    };

    /* skip config */
    for (mc_ind_t index = 1; index < scene->slide_count; ++index) {
        mc_ternary_status_t const ret =
            export_slide(timeline, index, encoder_context);

        if (ret == MC_TERNARY_STATUS_FAIL) {
            goto deconstructor;
        }
        else if (ret == MC_TERNARY_STATUS_FINISH) {
            break;
        }
    }

    mc_rwlock_reader_lock(timeline->state_lock);
    export_finish(timeline, NULL);
    mc_rwlock_reader_unlock(timeline->state_lock);

    encoder_context.frame = NULL;
    write_frame(timeline, NULL, encoder_context);
    av_write_trailer(fc);

deconstructor:
    if (fc && !(fc->oformat->flags & AVFMT_NOFILE)) {
        avio_close(fc->pb);
    }

    avformat_free_context(fc);
    av_packet_free(&packet_buffer);
    av_frame_free(&frame);
    avcodec_free_context(&ctx);
    sws_freeContext(sws);
}

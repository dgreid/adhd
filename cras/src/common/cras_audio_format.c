/* Copyright (c) 2012 The Chromium OS Authors. All rights reserved.
 * Use of this source code is governed by a BSD-style license that can be
 * found in the LICENSE file.
 */

#include <alsa/asoundlib.h>
#include <stddef.h>
#include <stdlib.h>
#include <syslog.h>

#include "cras_audio_format.h"

/* Create an audio format structure. */
struct cras_audio_format *cras_audio_format_create(snd_pcm_format_t format,
						   size_t frame_rate,
						   size_t num_channels)
{
	struct cras_audio_format *fmt;
	int i;

	fmt = (struct cras_audio_format *)calloc(1, sizeof(*fmt));
	if (fmt == NULL)
		return fmt;

	fmt->format = format;
	fmt->frame_rate = frame_rate;
	fmt->num_channels = num_channels;

	/* Initialize all channel position to -1(not set) */
	for (i = 0; i < CRAS_CH_MAX; i++)
		fmt->channel_layout[i] = -1;

	return fmt;
}

int cras_audio_format_set_channel_layout(struct cras_audio_format *format,
					 int8_t layout[CRAS_CH_MAX])
{
	int i;

	/* Check that the max channel index should not exceed the
	 * channel count set in format.
	 */
	for (i = 0; i < CRAS_CH_MAX; i++)
		if (layout[i] >= (int)format->num_channels)
			return -EINVAL;

	for (i = 0; i < CRAS_CH_MAX; i++)
		format->channel_layout[i] = layout[i];

	return 0;
}

/* Destroy an audio format struct created with cras_audio_format_crate. */
void cras_audio_format_destroy(struct cras_audio_format *fmt)
{
	free(fmt);
}

float** cras_channel_conv_matrix_alloc(size_t in_ch, size_t out_ch)
{
	size_t i;
	float **p;
	p = (float **)calloc(out_ch, sizeof(*p));
	if (p == NULL)
		return NULL;
	for (i = 0; i < out_ch; i++) {
		p[i] = (float *)calloc(in_ch, sizeof(*p[i]));
		if (p[i] == NULL)
			goto alloc_err;
	}
	return p;

alloc_err:
	if (p)
		cras_channel_conv_matrix_destroy(p, out_ch);
	return NULL;
}

void cras_channel_conv_matrix_destroy(float **p, size_t out_ch)
{
	size_t i;
	for (i = 0; i < out_ch; i ++)
		free(p[i]);
	free(p);
}

float **cras_channel_conv_matrix_create(const struct cras_audio_format *in,
					const struct cras_audio_format *out)
{
	int i;
	float **mtx;

	for (i = 0; i < CRAS_CH_MAX; i++) {
		if (in->channel_layout[i] >= (int)in->num_channels ||
		    out->channel_layout[i] >= (int)out->num_channels) {
			syslog(LOG_ERR, "Fail to create conversion matrix "
					"due to invalid channel layout");
			return NULL;
		}
	}

	mtx = cras_channel_conv_matrix_alloc(in->num_channels,
					     out->num_channels);

	/* For the in/out format pair which has the same set of channels
	 * in use, create a permutation matrix for them.
	 *
	 * TODO(hychao): support more conversion between channel layouts. For
	 * example allow {RL, RR} to fit to {SL, SR}.
	 */
	for (i = 0; i < CRAS_CH_MAX; i++) {
		if (in->channel_layout[i] == -1 &&
		    out->channel_layout[i] == -1)
			continue;
		if (in->channel_layout[i] != -1 &&
		    out->channel_layout[i] != -1)
			mtx[out->channel_layout[i]][in->channel_layout[i]] = 1;
		else
			goto fail;
	}

	return mtx;
fail:
	cras_channel_conv_matrix_destroy(mtx, out->num_channels);
	return NULL;
}

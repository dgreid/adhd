/* Copyright (c) 2013 The Chromium OS Authors. All rights reserved.
 * Use of this source code is governed by a BSD-style license that can be
 * found in the LICENSE file.
 */

#ifndef CRAS_LOOPBACK_IO_H_
#define CRAS_LOOPBACK_IO_H_

#include "cras_types.h"

struct cras_iodev;

/* Initializes an loopback iodev.  loopback iodevs are used when there are no
 * other iodevs available.  They give the attached streams a temporary place to
 * live until a new iodev becomes available.
 * Args:
 *    direciton - input or output.
 * Returns:
 *    A pointer to the newly created iodev if successful, NULL otherwise.
 */
struct cras_iodev *loopback_iodev_create(enum CRAS_STREAM_DIRECTION direction);

/* Destroys an loopback_iodev created with loopback_iodev_create. */
void loopback_iodev_destroy(struct cras_iodev *iodev);

/* Supplies samples to be looped back. */
int loopback_iodev_add_audio(struct cras_iodev *loopback_dev,
			     const uint8_t *audio,
			     unsigned int count,
			     struct cras_rstream *stream);

/* Set the format used for the loopback device.  This is set to match the output
 * that is being looped back. */
void loopback_iodev_set_format(struct cras_iodev *loopback_dev,
			       const struct cras_audio_format *fmt);

#endif /* CRAS_LOOPBACK_IO_H_ */

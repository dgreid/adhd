// Copyright (c) 2013 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

#include <stdio.h>
#include <stdlib.h>
#include <gtest/gtest.h>

extern "C" {
#include "cras_audio_area.h"
#include "cras_iodev.h"
#include "cras_loopback_iodev.h"
#include "cras_shm.h"
#include "cras_types.h"
}

namespace {

static const unsigned int kBufferFrames = 16384;
static const unsigned int kFrameBytes = 4;
static const unsigned int kBufferSize = kBufferFrames * kFrameBytes;

static cras_audio_area *dummy_audio_area;
static int (*loop_hook)(const uint8_t *frames, unsigned int nframes,
			const struct cras_audio_format *fmt);
static unsigned int cras_iodev_list_add_input_called;
static unsigned int cras_iodev_list_rm_input_called;
static unsigned int cras_iodev_list_add_dev_open_callback_called;
static void (*cras_iodev_list_add_dev_open_callback_cb)(struct cras_iodev *dev,
							int opened);

class LoopBackTestSuite : public testing::Test{
  protected:
    virtual void SetUp() {
      dummy_audio_area = (cras_audio_area*)calloc(
          1, sizeof(*dummy_audio_area) + sizeof(cras_channel_area) * 2);
      for (unsigned int i = 0; i < kBufferSize; i++) {
        buf_[i] = rand();
      }
      fmt_.frame_rate = 44100;
      fmt_.num_channels = 2;
      fmt_.format = SND_PCM_FORMAT_S16_LE;

      loop_in_ = loopback_iodev_create(LOOPBACK_POST_MIX_PRE_DSP);
      EXPECT_EQ(1, cras_iodev_list_add_input_called);
      EXPECT_EQ(1, cras_iodev_list_add_dev_open_callback_called);
      loop_in_->format = &fmt_;

      loop_hook = NULL;
      cras_iodev_list_add_input_called = 0;
      cras_iodev_list_rm_input_called = 0;
      cras_iodev_list_add_dev_open_callback_called = 0;
    }

    virtual void TearDown() {
      cras_iodev_list_add_dev_open_callback_called = 0;
      loopback_iodev_destroy(loop_in_);
      EXPECT_EQ(1, cras_iodev_list_rm_input_called);
      EXPECT_EQ(1, cras_iodev_list_add_dev_open_callback_called);
    }

    uint8_t buf_[kBufferSize];
    struct cras_audio_format fmt_;
    struct cras_iodev *loop_in_;
};

TEST_F(LoopBackTestSuite, OpenAndCloseDevice) {
  int rc;

  // Open loopback devices.
  rc = loop_in_->open_dev(loop_in_);
  EXPECT_EQ(0, rc);

  // Check device open status.
  rc = loop_in_->is_open(loop_in_);
  EXPECT_EQ(1, rc);

  // Check zero frames queued.
  rc = loop_in_->frames_queued(loop_in_);
  EXPECT_EQ(0, rc);

  // Close loopback devices.
  rc = loop_in_->close_dev(loop_in_);
  EXPECT_EQ(0, rc);

  // Check device open status.
  rc = loop_in_->is_open(loop_in_);
  EXPECT_EQ(0, rc);
}

TEST_F(LoopBackTestSuite, SimpleLoopback) {
  cras_audio_area *area;
  unsigned int nframes = 1024;
  unsigned int nread = 1024;
  int rc;

  loop_in_->open_dev(loop_in_);
  ASSERT_NE(reinterpret_cast<void *>(NULL), loop_hook);

  // Loopback callback for the hook.
  loop_hook(buf_, nframes, &fmt_);

  // Verify frames from loopback record.
  loop_in_->get_buffer(loop_in_, &area, &nread);
  EXPECT_EQ(nframes, nread);
  rc = memcmp(area->channels[0].buf, buf_, nframes * 4);
  EXPECT_EQ(0, rc);
  loop_in_->put_buffer(loop_in_, nread);

  // Check zero frames queued.
  rc = loop_in_->frames_queued(loop_in_);
  EXPECT_EQ(0, rc);

  loop_in_->close_dev(loop_in_);
}

/* Stubs */
extern "C" {

void cras_audio_area_config_buf_pointers(struct cras_audio_area *area,
                                         const struct cras_audio_format *fmt,
                                         uint8_t *base_buffer)
{
  dummy_audio_area->channels[0].buf = base_buffer;
}

void cras_iodev_free_audio_area(struct cras_iodev *iodev)
{
}

void cras_iodev_free_format(struct cras_iodev *iodev)
{
}

void cras_iodev_init_audio_area(struct cras_iodev *iodev, int num_channels)
{
  iodev->area = dummy_audio_area;
}

void cras_iodev_add_node(struct cras_iodev *iodev, struct cras_ionode *node)
{
}

void cras_iodev_set_active_node(struct cras_iodev *iodev,
                                struct cras_ionode *node)
{
}

void cras_iodev_register_pre_dsp_hook(struct cras_iodev *iodev,
				      loopback_hook_t loop_cb)
{
  loop_hook = loop_cb;
  return 0;
}

void cras_iodev_register_post_dsp_hook(struct cras_iodev *iodev,
				       loopback_hook_t loop_cb)
{
  loop_hook = loop_cb;
  return 0;
}

int cras_iodev_list_add_input(struct cras_iodev *input)
{
  cras_iodev_list_add_input_called++;
  return 0;
}

int cras_iodev_list_rm_input(struct cras_iodev *input)
{
  cras_iodev_list_rm_input_called++;
  return 0;
}

int cras_iodev_list_add_dev_open_callback(struct cras_iodev *dev,
					  device_open_callback_t cb)
{
	cras_iodev_list_add_dev_open_callback_called++;
	cras_iodev_list_add_dev_open_callback_cb = cb;
}

}  // extern "C"

}  //  namespace

int main(int argc, char **argv) {
  ::testing::InitGoogleTest(&argc, argv);
  return RUN_ALL_TESTS();
}

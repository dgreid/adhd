// Copyright (c) 2013 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

#include <stdio.h>
#include <stdint.h>
#include <gtest/gtest.h>

extern "C" {

#include "a2dp-codecs.h"
#include "audio_thread.h"
#include "audio_thread_log.h"
#include "cras_bt_transport.h"
#include "cras_iodev.h"
#include "cras_iodev_list.h"

#include "cras_a2dp_iodev.h"
}

#define FAKE_DEVICE_NAME "fake device name"
#define FAKE_OBJECT_PATH "/fake/obj/path"

#define MAX_A2DP_ENCODE_CALLS 2
#define MAX_A2DP_WRITE_CALLS 2

static struct cras_bt_transport *fake_transport;
static struct cras_bt_device *fake_device;
static cras_audio_format format;
static size_t cras_iodev_list_add_output_called;
static size_t cras_iodev_list_rm_output_called;
static size_t cras_iodev_add_node_called;
static size_t cras_iodev_rm_node_called;
static size_t cras_iodev_set_active_node_called;
static size_t cras_bt_transport_acquire_called;
static size_t cras_bt_transport_configuration_called;
static size_t cras_bt_transport_release_called;
static size_t init_a2dp_called;
static int init_a2dp_return_val;
static size_t destroy_a2dp_called;
static size_t drain_a2dp_called;
static size_t a2dp_block_size_called;
static size_t a2dp_queued_frames_val;
static size_t cras_iodev_free_format_called;
static size_t cras_iodev_free_dsp_called;
static int pcm_buf_size_val[MAX_A2DP_ENCODE_CALLS];
static unsigned int a2dp_encode_processed_bytes_val[MAX_A2DP_ENCODE_CALLS];
static unsigned int a2dp_encode_index;
static int a2dp_write_return_val[MAX_A2DP_WRITE_CALLS];
static unsigned int a2dp_write_index;
static thread_callback write_callback;
static void *write_callback_data;

void ResetStubData() {
  cras_iodev_list_add_output_called = 0;
  cras_iodev_list_rm_output_called = 0;
  cras_iodev_add_node_called = 0;
  cras_iodev_rm_node_called = 0;
  cras_iodev_set_active_node_called = 0;
  cras_bt_transport_acquire_called = 0;
  cras_bt_transport_configuration_called = 0;
  cras_bt_transport_release_called = 0;
  init_a2dp_called = 0;
  init_a2dp_return_val = 0;
  destroy_a2dp_called = 0;
  drain_a2dp_called = 0;
  a2dp_block_size_called = 0;
  a2dp_queued_frames_val = 0;
  cras_iodev_free_format_called = 0;
  cras_iodev_free_dsp_called = 0;
  memset(a2dp_encode_processed_bytes_val, 0,
         sizeof(a2dp_encode_processed_bytes_val));
  a2dp_encode_index = 0;
  a2dp_write_index = 0;

  fake_transport = reinterpret_cast<struct cras_bt_transport *>(0x123);
  fake_device = NULL;

  write_callback = NULL;
}

int iodev_set_format(struct cras_iodev *iodev,
                     struct cras_audio_format *fmt)
{
  fmt->format = SND_PCM_FORMAT_S16_LE;
  fmt->num_channels = 2;
  fmt->frame_rate = 44100;
  iodev->format = fmt;
  return 0;
}

namespace {

static struct timespec time_now;

TEST(A2dpIoInit, InitializeA2dpIodev) {
  struct cras_iodev *iodev;

  atlog = (audio_thread_event_log *)calloc(1, sizeof(audio_thread_event_log));

  ResetStubData();

  iodev = a2dp_iodev_create(fake_transport, NULL);

  ASSERT_NE(iodev, (void *)NULL);
  ASSERT_EQ(iodev->direction, CRAS_STREAM_OUTPUT);
  ASSERT_EQ(1, cras_bt_transport_configuration_called);
  ASSERT_EQ(1, init_a2dp_called);
  ASSERT_EQ(1, cras_iodev_list_add_output_called);
  ASSERT_EQ(1, cras_iodev_add_node_called);
  ASSERT_EQ(1, cras_iodev_set_active_node_called);

  /* Assert iodev name matches the object path when bt device is NULL */
  ASSERT_STREQ(FAKE_OBJECT_PATH, iodev->info.name);

  a2dp_iodev_destroy(iodev);

  ASSERT_EQ(1, cras_iodev_list_rm_output_called);
  ASSERT_EQ(1, cras_iodev_rm_node_called);
  ASSERT_EQ(1, destroy_a2dp_called);
  ASSERT_EQ(1, cras_iodev_free_dsp_called);

  /* Assert iodev name matches the bt device's name */
  fake_device = reinterpret_cast<struct cras_bt_device *>(0x456);
  iodev = a2dp_iodev_create(fake_transport, NULL);
  ASSERT_STREQ(FAKE_DEVICE_NAME, iodev->info.name);

  a2dp_iodev_destroy(iodev);
}

TEST(A2dpIoInit, InitializeFail) {
  struct cras_iodev *iodev;

  ResetStubData();

  init_a2dp_return_val = -1;
  iodev = a2dp_iodev_create(fake_transport, NULL);

  ASSERT_EQ(iodev, (void *)NULL);
  ASSERT_EQ(1, cras_bt_transport_configuration_called);
  ASSERT_EQ(1, init_a2dp_called);
  ASSERT_EQ(0, cras_iodev_list_add_output_called);
  ASSERT_EQ(0, cras_iodev_add_node_called);
  ASSERT_EQ(0, cras_iodev_set_active_node_called);
  ASSERT_EQ(0, cras_iodev_rm_node_called);
}

TEST(A2dpIoInit, OpenIodev) {
  struct cras_iodev *iodev;

  ResetStubData();
  iodev = a2dp_iodev_create(fake_transport, NULL);

  iodev_set_format(iodev, &format);
  iodev->open_dev(iodev);

  ASSERT_EQ(1, cras_bt_transport_acquire_called);

  iodev->close_dev(iodev);
  ASSERT_EQ(1, cras_bt_transport_release_called);
  ASSERT_EQ(1, drain_a2dp_called);
  ASSERT_EQ(1, cras_iodev_free_format_called);

  a2dp_iodev_destroy(iodev);
}

TEST(A2dpIoInit, GetPutBuffer) {
  struct cras_iodev *iodev;
  uint8_t *buf1, *buf2, *buf3;
  unsigned frames;

  ResetStubData();
  iodev = a2dp_iodev_create(fake_transport, NULL);

  iodev_set_format(iodev, &format);
  iodev->open_dev(iodev);
  ASSERT_NE(write_callback, (void *)NULL);

  frames = 256;
  iodev->get_buffer(iodev, &buf1, &frames);
  ASSERT_EQ(256, frames);

  /* Test 100 frames(400 bytes) put and all processed. */
  a2dp_encode_processed_bytes_val[0] = 400;
  a2dp_write_index = 0;
  a2dp_write_return_val[0] = 400;
  iodev->put_buffer(iodev, 100);
  write_callback(write_callback_data);
  ASSERT_EQ(400, pcm_buf_size_val[0]);
  ASSERT_EQ(1, a2dp_block_size_called);

  iodev->get_buffer(iodev, &buf2, &frames);
  ASSERT_EQ(256, frames);

  /* Assert buf2 points to the same position as buf1 */
  ASSERT_EQ(400, buf2 - buf1);

  /* Test 100 frames(400 bytes) put, only 360 bytes processed,
   * 40 bytes left in pcm buffer.
   */
  a2dp_encode_index = 0;
  a2dp_encode_processed_bytes_val[0] = 360;
  a2dp_encode_processed_bytes_val[1] = 0;
  a2dp_write_index = 0;
  a2dp_write_return_val[0] = 360;
  a2dp_write_return_val[1] = 0;
  iodev->put_buffer(iodev, 100);
  write_callback(write_callback_data);
  ASSERT_EQ(400, pcm_buf_size_val[0]);
  ASSERT_EQ(40, pcm_buf_size_val[1]);
  ASSERT_EQ(2, a2dp_block_size_called);

  iodev->get_buffer(iodev, &buf3, &frames);

  /* Existing buffer not completed processed, assert new buffer starts from
   * current write pointer.
   */
  ASSERT_EQ(256, frames);
  ASSERT_EQ(800, buf3 - buf1);

  a2dp_iodev_destroy(iodev);
}

TEST(A2dpIoInif, FramesQueued) {
  struct cras_iodev *iodev;
  uint8_t *buf;
  unsigned frames;

  ResetStubData();
  iodev = a2dp_iodev_create(fake_transport, NULL);

  iodev_set_format(iodev, &format);
  time_now.tv_sec = 0;
  time_now.tv_nsec = 0;
  iodev->open_dev(iodev);
  ASSERT_NE(write_callback, (void *)NULL);

  frames = 256;
  iodev->get_buffer(iodev, &buf, &frames);
  ASSERT_EQ(256, frames);

  /* Put 100 frames, proccessed 400 bytes to a2dp buffer.
   * Assume 200 bytes written out, queued 50 frames in a2dp buffer.
   */
  a2dp_encode_processed_bytes_val[0] = 400;
  a2dp_encode_processed_bytes_val[1] = 0;
  a2dp_write_return_val[0] = 200;
  a2dp_write_return_val[1] = 0;
  a2dp_queued_frames_val = 50;
  time_now.tv_sec = 0;
  time_now.tv_nsec = 1000000;
  iodev->put_buffer(iodev, 100);
  write_callback(write_callback_data);
  ASSERT_EQ(1, a2dp_block_size_called);
  ASSERT_EQ(6, iodev->frames_queued(iodev));

  /* After 1ms, 44 more frames consumed but no more frames written yet.
   */
  time_now.tv_sec = 0;
  time_now.tv_nsec = 2000000;
  ASSERT_EQ(0, iodev->frames_queued(iodev));

  /* Queued frames and new put buffer are all written */
  a2dp_encode_processed_bytes_val[0] = 400;
  a2dp_queued_frames_val = 50;
  a2dp_encode_index = 0;
  a2dp_write_return_val[0] = 400;
  a2dp_write_index = 0;

  /* After 1 more ms, expect total 132 frames consumed, result 68
   * frames of virtual buffer after total 200 frames put.
   */
  time_now.tv_sec = 0;
  time_now.tv_nsec = 3000000;
  iodev->put_buffer(iodev, 100);
  write_callback(write_callback_data);
  ASSERT_EQ(400, pcm_buf_size_val[0]);
  ASSERT_EQ(18, iodev->frames_queued(iodev));
}

} // namespace

int main(int argc, char **argv) {
  ::testing::InitGoogleTest(&argc, argv);
  return RUN_ALL_TESTS();
}

extern "C" {

int cras_bt_transport_configuration(const struct cras_bt_transport *transport,
                                    void *configuration, int len)
{
  cras_bt_transport_configuration_called++;
  return 0;
}

int cras_bt_transport_acquire(struct cras_bt_transport *transport)
{
  cras_bt_transport_acquire_called++;
  return 0;
}

int cras_bt_transport_release(struct cras_bt_transport *transport)
{
  cras_bt_transport_release_called++;
  return 0;
}

int cras_bt_transport_fd(const struct cras_bt_transport *transport)
{
  return 0;
}

const char *cras_bt_transport_object_path(
		const struct cras_bt_transport *transport)
{
  return FAKE_OBJECT_PATH;
}

uint16_t cras_bt_transport_write_mtu(const struct cras_bt_transport *transport)
{
  /* 256 frames of 16 bit stereo, plus header size */
  return 1024 + 13;
}


void cras_iodev_free_format(struct cras_iodev *iodev)
{
  cras_iodev_free_format_called++;
}

void cras_iodev_free_dsp(struct cras_iodev *iodev)
{
  cras_iodev_free_dsp_called++;
}

// Cras iodev
void cras_iodev_add_node(struct cras_iodev *iodev, struct cras_ionode *node)
{
  cras_iodev_add_node_called++;
  iodev->nodes = node;
}

void cras_iodev_rm_node(struct cras_iodev *iodev, struct cras_ionode *node)
{
  cras_iodev_rm_node_called++;
  iodev->nodes = NULL;
}

void cras_iodev_set_active_node(struct cras_iodev *iodev,
				struct cras_ionode *node)
{
  cras_iodev_set_active_node_called++;
  iodev->active_node = node;
}

//  From iodev list.
int cras_iodev_list_add_output(struct cras_iodev *output)
{
  cras_iodev_list_add_output_called++;
  return 0;
}

int cras_iodev_list_rm_output(struct cras_iodev *dev)
{
  cras_iodev_list_rm_output_called++;
  return 0;
}

// From cras_bt_transport
struct cras_bt_device *cras_bt_transport_device(
	const struct cras_bt_transport *transport)
{
  return fake_device;
}

// From cras_bt_device
const char *cras_bt_device_name(const struct cras_bt_device *device)
{
  return FAKE_DEVICE_NAME;
}

int init_a2dp(struct a2dp_info *a2dp, a2dp_sbc_t *sbc)
{
  init_a2dp_called++;
  return init_a2dp_return_val;
}

void destroy_a2dp(struct a2dp_info *a2dp)
{
  destroy_a2dp_called++;
}

int a2dp_codesize(struct a2dp_info *a2dp)
{
  return 512;
}

int a2dp_block_size(struct a2dp_info *a2dp, int encoded_bytes)
{
  a2dp_block_size_called++;

  // Assumes a2dp block size is 1:1 before/after encode.
  return encoded_bytes;
}

int a2dp_queued_frames(struct a2dp_info *a2dp)
{
  return a2dp_queued_frames_val;
}

void a2dp_drain(struct a2dp_info *a2dp)
{
  drain_a2dp_called++;
}

int a2dp_encode(struct a2dp_info *a2dp, const void *pcm_buf, int pcm_buf_size,
                int format_bytes, size_t link_mtu) {
  unsigned int processed;

  if (a2dp_encode_index == MAX_A2DP_ENCODE_CALLS)
    return 0;
  processed = a2dp_encode_processed_bytes_val[a2dp_encode_index];
  pcm_buf_size_val[a2dp_encode_index] = pcm_buf_size;
  a2dp_encode_index++;
  return processed;
}

int a2dp_write(struct a2dp_info *a2dp, int stream_fd, size_t link_mtu) {
  return a2dp_write_return_val[a2dp_write_index++];;
}

int clock_gettime(clockid_t clk_id, struct timespec *tp) {
  *tp = time_now;
  return 0;
}

// From audio_thread
struct audio_thread_event_log *atlog;

void audio_thread_add_write_callback(int fd, thread_callback cb, void *data) {
  write_callback = cb;
  write_callback_data = data;
}

void audio_thread_rm_callback(int fd) {
}

void audio_thread_enable_callback(int fd, int enabled) {
}

}

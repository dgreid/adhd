// Copyright 2017 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

#include <memory>
#include <stdint.h>
#include <stdio.h>
#include <time.h>

#include <gtest/gtest.h>

extern "C" {
#include "dev_io.h" // tested
#include "dev_stream.h" // tested
#include "cras_rstream.h" // stubbed
#include "cras_iodev.h" // stubbed
#include "cras_shm.h"
#include "cras_types.h"
#include "utlist.h"

struct audio_thread_event_log* atlog;
}

#include "dev_io_stubs.h"
#include "iodev_stub.h"
#include "rstream_stub.h"

#define FAKE_POLL_FD 33

namespace {

class TimingSuite : public testing::Test{
 protected:
  virtual void SetUp() {
    atlog = static_cast<audio_thread_event_log*>(calloc(1, sizeof(*atlog)));
    iodev_stub_reset();
    rstream_stub_reset();
  }

  virtual void TearDown() {
    free(atlog);
  }

  timespec SingleInputDevNextWake(
      size_t dev_cb_threshold,
      size_t dev_level,
      const timespec* level_timestamp,
      cras_audio_format* dev_format,
      const std::vector<StreamPtr>& streams) {
    struct open_dev* dev_list_ = NULL;

    DevicePtr dev = create_device(CRAS_STREAM_INPUT, dev_cb_threshold,
                                  dev_format, CRAS_NODE_TYPE_MIC);
    DL_APPEND(dev_list_, dev->odev.get());

    for (auto const& stream : streams) {
      add_stream_to_dev(dev->dev, stream);
    }

    // Set response for frames_queued.
    iodev_stub_frames_queued(dev->dev.get(), dev_level, *level_timestamp);

    dev_io_send_captured_samples(dev_list_);

    struct timespec dev_time;
    dev_time.tv_sec = level_timestamp->tv_sec + 500; // Far in the future.
    dev_io_next_input_wake(&dev_list_, &dev_time);
    return dev_time;
  }
};

// One device, one stream, write a callback of data and check the sleep time is
// one more wakeup interval.
TEST_F(TimingSuite, WaitAfterFill) {
  const size_t cb_threshold = 480;

  cras_audio_format format;
  fill_audio_format(&format, 48000);

  StreamPtr stream =
      create_stream(1, 1, CRAS_STREAM_INPUT, cb_threshold, &format);
  // rstream's next callback is now and there is enough data to fill.
  struct timespec start;
  clock_gettime(CLOCK_MONOTONIC_RAW, &start);
  stream->rstream->next_cb_ts = start;
  AddFakeDataToStream(stream.get(), 480);

  std::vector<StreamPtr> streams;
  streams.emplace_back(std::move(stream));
  timespec dev_time = SingleInputDevNextWake(cb_threshold, 0, &start,
                                             &format, streams);

  // The next callback should be scheduled 10ms in the future.
  // And the next wake up should reflect the only attached stream.
  EXPECT_EQ(dev_time.tv_sec, streams[0]->rstream->next_cb_ts.tv_sec);
  EXPECT_EQ(dev_time.tv_nsec, streams[0]->rstream->next_cb_ts.tv_nsec);
}

// One device(48k), one stream(44.1k), write a callback of data and check that
// the sleep time is correct when doing SRC.
TEST_F(TimingSuite, WaitAfterFillSRC) {
  cras_audio_format dev_format;
  fill_audio_format(&dev_format, 48000);
  cras_audio_format stream_format;
  fill_audio_format(&stream_format, 44100);

  StreamPtr stream =
      create_stream(1, 1, CRAS_STREAM_INPUT, 441, &stream_format);
  // rstream's next callback is now and there is enough data to fill.
  struct timespec start;
  clock_gettime(CLOCK_MONOTONIC_RAW, &start);
  stream->rstream->next_cb_ts = start;
  AddFakeDataToStream(stream.get(), 441);

  std::vector<StreamPtr> streams;
  streams.emplace_back(std::move(stream));
  timespec dev_time = SingleInputDevNextWake(480, 0, &start,
                                             &dev_format, streams);

  // The next callback should be scheduled 10ms in the future.
  struct timespec delta;
  subtract_timespecs(&dev_time, &start, &delta);
  EXPECT_LT(9900 * 1000, delta.tv_nsec);
  EXPECT_GT(10100 * 1000, delta.tv_nsec);
}

// One device, two streams. One stream is ready the other still needs data.
// Checks that the sleep interval is based on the time the device will take to
// supply the needed samples for stream2.
TEST_F(TimingSuite, WaitTwoStreamsSameFormat) {
  const size_t cb_threshold = 480;

  cras_audio_format format;
  fill_audio_format(&format, 48000);

  // stream1's next callback is now and there is enough data to fill.
  StreamPtr stream1 =
      create_stream(1, 1, CRAS_STREAM_INPUT, cb_threshold, &format);
  struct timespec start;
  clock_gettime(CLOCK_MONOTONIC_RAW, &start);
  stream1->rstream->next_cb_ts = start;
  AddFakeDataToStream(stream1.get(), cb_threshold);

  // stream2 is only half full.
  StreamPtr stream2  =
      create_stream(1, 1, CRAS_STREAM_INPUT, cb_threshold, &format);
  stream2->rstream->next_cb_ts = start;
  AddFakeDataToStream(stream2.get(), 240);

  std::vector<StreamPtr> streams;
  streams.emplace_back(std::move(stream1));
  streams.emplace_back(std::move(stream2));
  timespec dev_time = SingleInputDevNextWake(cb_threshold, 0, &start,
                                             &format, streams);

  // Should wait for approximately 5 milliseconds for 240 samples at 48k.
  struct timespec delta2;
  subtract_timespecs(&dev_time, &start, &delta2);
  EXPECT_LT(4900 * 1000, delta2.tv_nsec);
  EXPECT_GT(5100 * 1000, delta2.tv_nsec);
}

// One device(44.1), two streams(44.1, 48). One stream is ready the other still
// needs data. Checks that the sleep interval is based on the time the device
// will take to supply the needed samples for stream2, stream2 is sample rate
// converted from the 44.1k device to the 48k stream.
TEST_F(TimingSuite, WaitTwoStreamsDifferentRates) {
  cras_audio_format s1_format, s2_format;
  fill_audio_format(&s1_format, 44100);
  fill_audio_format(&s2_format, 48000);

  // stream1's next callback is now and there is enough data to fill.
  StreamPtr stream1 =
      create_stream(1, 1, CRAS_STREAM_INPUT, 441, &s1_format);
  struct timespec start;
  clock_gettime(CLOCK_MONOTONIC_RAW, &start);
  stream1->rstream->next_cb_ts = start;
  AddFakeDataToStream(stream1.get(), 441);
  // stream2's next callback is now but there is only half a callback of data.
  StreamPtr stream2  =
      create_stream(1, 1, CRAS_STREAM_INPUT, 480, &s2_format);
  stream2->rstream->next_cb_ts = start;
  AddFakeDataToStream(stream2.get(), 240);

  std::vector<StreamPtr> streams;
  streams.emplace_back(std::move(stream1));
  streams.emplace_back(std::move(stream2));
  timespec dev_time = SingleInputDevNextWake(441, 0, &start,
                                             &s1_format, streams);

  // Should wait for approximately 5 milliseconds for 240 48k samples from the
  // 44.1k device.
  struct timespec delta2;
  subtract_timespecs(&dev_time, &start, &delta2);
  EXPECT_LT(4900 * 1000, delta2.tv_nsec);
  EXPECT_GT(5100 * 1000, delta2.tv_nsec);
}

// One device, two streams. Both streams get a full callback of data and the
// device has enough samples for the next callback already. Checks that the
// shorter of the two streams times is used for the next sleep interval.
TEST_F(TimingSuite, WaitTwoStreamsDifferentWakeupTimes) {
  cras_audio_format s1_format, s2_format;
  fill_audio_format(&s1_format, 44100);
  fill_audio_format(&s2_format, 48000);

  struct timespec start;
  clock_gettime(CLOCK_MONOTONIC_RAW, &start);

  // stream1's next callback is in 3ms.
  StreamPtr stream1 =
      create_stream(1, 1, CRAS_STREAM_INPUT, 441, &s1_format);
  stream1->rstream->next_cb_ts = start;
  const timespec three_millis = { 0, 3 * 1000 * 1000 };
  add_timespecs(&stream1->rstream->next_cb_ts, &three_millis);
  AddFakeDataToStream(stream1.get(), 441);
  // stream2 is also ready next cb in 5ms..
  StreamPtr stream2  =
      create_stream(1, 1, CRAS_STREAM_INPUT, 480, &s2_format);
  stream2->rstream->next_cb_ts = start;
  const timespec five_millis = { 0, 5 * 1000 * 1000 };
  add_timespecs(&stream2->rstream->next_cb_ts, &five_millis);
  AddFakeDataToStream(stream1.get(), 480);

  std::vector<StreamPtr> streams;
  streams.emplace_back(std::move(stream1));
  streams.emplace_back(std::move(stream2));
  timespec dev_time = SingleInputDevNextWake(441, 441, &start,
                                             &s1_format, streams);

  // Should wait for approximately 3 milliseconds for stream 1 first.
  struct timespec delta2;
  subtract_timespecs(&dev_time, &start, &delta2);
  EXPECT_LT(2900 * 1000, delta2.tv_nsec);
  EXPECT_GT(3100 * 1000, delta2.tv_nsec);
}

// One hotword stream attaches to hotword device. Input data has copied from
// device to stream but total number is less than cb_threshold. Hotword stream
// should be scheduled wake base on the samples needed to fill full shm.
TEST_F(TimingSuite, HotwordStreamUseDevTiming) {
  cras_audio_format fmt;
  fill_audio_format(&fmt, 48000);

  struct timespec start, delay;
  clock_gettime(CLOCK_MONOTONIC_RAW, &start);

  StreamPtr stream =
      create_stream(1, 1, CRAS_STREAM_INPUT, 240, &fmt);
  stream->rstream->flags = HOTWORD_STREAM;
  stream->rstream->next_cb_ts = start;
  delay.tv_sec = 0;
  delay.tv_nsec = 3 * 1000 * 1000;
  add_timespecs(&stream->rstream->next_cb_ts, &delay);

  // Add fake data to stream and device so its slightly less than cb_threshold.
  // Expect to wait for samples to fill the full buffer (480 - 192) frames
  // instead of using the next_cb_ts.
  AddFakeDataToStream(stream.get(), 192);
  std::vector<StreamPtr> streams;
  streams.emplace_back(std::move(stream));
  timespec dev_time = SingleInputDevNextWake(4096, 0, &start,
                                             &fmt, streams);
  struct timespec delta;
  subtract_timespecs(&dev_time, &start, &delta);
  // 288 frames worth of time = 6 ms.
  EXPECT_EQ(6 * 1000 * 1000, delta.tv_nsec);
}

// One hotword stream attaches to hotword device. Input data burst to a number
// larger than cb_threshold. In this case stream fd is used to poll for next
// wake. And the dev wake time is unchanged from the default 20 seconds limit.
TEST_F(TimingSuite, HotwordStreamBulkData) {
  cras_audio_format fmt;
  fill_audio_format(&fmt, 48000);

  struct timespec start;
  clock_gettime(CLOCK_MONOTONIC_RAW, &start);

  StreamPtr stream =
      create_stream(1, 1, CRAS_STREAM_INPUT, 240, &fmt);
  stream->rstream->flags = HOTWORD_STREAM;
  stream->rstream->next_cb_ts = start;

  AddFakeDataToStream(stream.get(), 480);
  std::vector<StreamPtr> streams;
  streams.emplace_back(std::move(stream));
  timespec dev_time = SingleInputDevNextWake(4096, 7000, &start,
                                             &fmt, streams);

  int poll_fd = dev_stream_poll_stream_fd(streams[0]->dstream.get());
  EXPECT_EQ(FAKE_POLL_FD, poll_fd);

  struct timespec delta;
  subtract_timespecs(&dev_time, &start, &delta);
  EXPECT_LT(19, delta.tv_sec);
  EXPECT_GT(21, delta.tv_sec);
}

/* Stubs */
extern "C" {

int cras_server_metrics_longest_fetch_delay(unsigned delay_msec) {
  return 0;
}

}  // extern "C"

}  //  namespace

int main(int argc, char** argv) {
  ::testing::InitGoogleTest(&argc, argv);
  return RUN_ALL_TESTS();
}

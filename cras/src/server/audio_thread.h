/* Copyright (c) 2012 The Chromium OS Authors. All rights reserved.
 * Use of this source code is governed by a BSD-style license that can be
 * found in the LICENSE file.
 */

#ifndef AUDIO_THREAD_H_
#define AUDIO_THREAD_H_

#include <pthread.h>
#include <stdint.h>

struct cras_iodev;

/* Linked list of streams of audio from/to a client. */
struct cras_io_stream {
	struct cras_rstream *stream;
	int fd; /* cached here due to frequent access */
	int mixed; /* Was this stream mixed already? */
	struct cras_io_stream *prev, *next;
};

/* Hold communication pipes and pthread info for a thread used to play or record
 * audio.  This maps 1 to 1 with IO devices.
 *    odev - The output device to attach this thread to, NULL if none.
 *    idev - The input device to attach this thread to, NULL if none.
 *    to_thread_fds - Send a message from main to running thread.
 *    to_main_fds - Send a message to main from running thread.
 *    tid - Thread ID of the running playback/capture thread.
 *    started - Non-zero if the thread has started successfully.
 *    sleep_correction_frames - Number of frames to adjust sleep time by.  This
 *      is adjusted based on sleeping too long or short so that the sleep
 *      interval tracks toward the targeted number of frames.
 *    remaining_target - For capture the amount of frames that will be left
 *        after a read is performed. Sleep this many frames past the buffer
 *        size to be sure at least the buffer size is captured when the audio
 *        thread wakes up.
 *    streams - List of audio streams serviced by this thread.
 */
struct audio_thread {
	struct cras_iodev *output_dev;
	struct cras_iodev *input_dev;
	int to_thread_fds[2];
	int to_main_fds[2];
	pthread_t tid;
	int started;
	int sleep_correction_frames;
	unsigned int remaining_target;
	struct cras_io_stream *streams;
};

/* Messages that can be sent from the main context to the audio thread. */
enum AUDIO_THREAD_COMMAND {
	AUDIO_THREAD_ADD_STREAM,
	AUDIO_THREAD_RM_STREAM,
	AUDIO_THREAD_STOP,
};

struct audio_thread_msg {
	size_t length;
	enum AUDIO_THREAD_COMMAND id;
};

struct audio_thread_add_rm_stream_msg {
	struct audio_thread_msg header;
	struct cras_rstream *stream;
};

/* Creates an audio thread.
 * Args:
 *    iodev - The iodev to attach this thread to.
 * Returns:
 *    A pointer to the newly create audio thread.  It has been allocated from
 *    the heap and must be freed by calling audio_thread_destroy().  NULL on
 *    error.
 */
struct audio_thread *audio_thread_create(struct cras_iodev *iodev);

/* Starts a thread created with audio_thread_create.
 * Args:
 *    thread - The thread to start.
 * Returns:
 *    0 on success, return code from pthread_crate on failure.
 */
int audio_thread_start(struct audio_thread *thread);

/* Frees an audio thread created with audio_thread_create(). */
void audio_thread_destroy(struct audio_thread *thread);

/* Add a stream to the thread.
 * Args:
 *    thread - a pointer to the audio thread.
 *    stream - the new stream to add.
 * Returns:
 *    zero on success, negative error otherwise.
 */
int audio_thread_add_stream(struct audio_thread *thread,
			    struct cras_rstream *stream);

/* Remove a stream from the thread.
 * Args:
 *    thread - a pointer to the audio thread.
 *    stream - the new stream to remove.
 * Returns:
 *    one if there are streams remaining, zero if not, negative if error.
 */
int audio_thread_rm_stream(struct audio_thread *thread,
			   struct cras_rstream *stream);

/* Remove all streams from the thread.
 * Args:
 *    thread - a pointer to the audio thread.
 */
void audio_thread_rm_all_streams(struct audio_thread *thread);

#endif /* AUDIO_THREAD_H_ */

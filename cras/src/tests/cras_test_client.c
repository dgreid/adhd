/* Copyright (c) 2012 The Chromium OS Authors. All rights reserved.
 * Use of this source code is governed by a BSD-style license that can be
 * found in the LICENSE file.
 */

#include <alsa/asoundlib.h>
#include <getopt.h>
#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <sys/select.h>
#include <unistd.h>

#include "cras_sbc_codec.h"
#include "cras_client.h"
#include "cras_types.h"
#include "cras_util.h"

#define PLAYBACK_CB_THRESHOLD (480)
#define PLAYBACK_BUFFER_SIZE (4800)

#define BUF_SIZE 32768

static const size_t MAX_IODEVS = 10; /* Max devices to print out. */
static const size_t MAX_IONODES = 20; /* Max ionodes to print out. */
static const size_t MAX_ATTACHED_CLIENTS = 10; /* Max clients to print out. */

static uint8_t *file_buf;
static size_t file_buf_size;
static size_t file_buf_read_offset;
static struct timespec last_latency;
static int show_latency;
static int keep_looping = 1;
static int exit_after_done_playing = 1;
static size_t duration_frames;
static int full_frames;
uint32_t min_cb_level = PLAYBACK_CB_THRESHOLD;

static struct cras_audio_codec *capture_codec;
static struct cras_audio_codec *playback_codec;
static unsigned char cap_buf[BUF_SIZE];

struct cras_audio_format *aud_format;

static void check_stream_terminate(size_t frames)
{
	if (duration_frames) {
		if (duration_frames <= frames)
			keep_looping = 0;
		else
			duration_frames -= frames;
	}
}

/* Run from callback thread. */
static int got_samples(struct cras_client *client, cras_stream_id_t stream_id,
		       uint8_t *samples, size_t frames,
		       const struct timespec *sample_time, void *arg)
{
	int *fd = (int *)arg;
	int ret;
	int write_size;
	int processed_bytes, frame_bytes;
	size_t encoded;

	check_stream_terminate(frames);

	cras_client_calc_capture_latency(sample_time, &last_latency);

	frame_bytes = cras_client_format_bytes_per_frame(aud_format);
	write_size = frames * frame_bytes;

	if (capture_codec) {
		processed_bytes = capture_codec->encode(capture_codec, samples,
					       write_size, cap_buf, BUF_SIZE,
					       &encoded);
		if (processed_bytes <= 0 || processed_bytes > write_size) {
			keep_looping = 0;
			return EOF;
		}

		ret = write(*fd, cap_buf, encoded);
		if (ret != encoded)
			printf("Error writing file\n");

		return processed_bytes / frame_bytes;
	} else {
		ret = write(*fd, samples, write_size);
		if (ret != write_size)
			printf("Error writing file\n");
		return frames;
	}
}

/* Run from callback thread. */
static int put_samples(struct cras_client *client, cras_stream_id_t stream_id,
		       uint8_t *samples, size_t frames,
		       const struct timespec *sample_time, void *arg)
{
	size_t this_size, decoded;
	snd_pcm_uframes_t avail;
	uint32_t frame_bytes = cras_client_format_bytes_per_frame(aud_format);

	if (file_buf_read_offset >= file_buf_size) {
		if (exit_after_done_playing)
			keep_looping = 0;
		return EOF;
	}

	check_stream_terminate(frames);

	if (frames < min_cb_level)
		printf("req for only %zu - %d min\n", frames, min_cb_level);
	avail = frames * frame_bytes;

	this_size = file_buf_size - file_buf_read_offset;
	if (this_size > avail)
		this_size = avail;

	if (full_frames && this_size > min_cb_level * frame_bytes)
		this_size = min_cb_level * frame_bytes;

	cras_client_calc_playback_latency(sample_time, &last_latency);

	if (playback_codec) {
		this_size = playback_codec->decode(playback_codec,
				       file_buf + file_buf_read_offset,
				       file_buf_size - file_buf_read_offset,
				       samples, this_size, &decoded);

		file_buf_read_offset += this_size;
		if (this_size == 0) {
			printf("stop looping\n");
			keep_looping = 0;
			return EOF;
		}
		return decoded / frame_bytes;
	} else {
		memcpy(samples, file_buf + file_buf_read_offset, this_size);
		file_buf_read_offset += this_size;
		return this_size / frame_bytes;
	}
}

/* Run from callback thread. */
static int unified_samples(struct cras_client *client,
			   cras_stream_id_t stream_id,
			   uint8_t *captured_samples,
			   uint8_t *playback_samples,
			   unsigned int frames,
			   const struct timespec *captured_time,
			   const struct timespec *playback_time,
			   void *user_arg)
{
	unsigned int frame_bytes;

	frame_bytes = cras_client_format_bytes_per_frame(aud_format);
	memcpy(playback_samples, captured_samples, frames * frame_bytes);
	return frames;
}

static int stream_error(struct cras_client *client,
			cras_stream_id_t stream_id,
			int err,
			void *arg)
{
	printf("Stream error %d\n", err);
	keep_looping = 0;
	return 0;
}

static void print_last_latency()
{
	if (last_latency.tv_sec > 0 || last_latency.tv_nsec > 0)
		printf("%u.%09u\n", (unsigned)last_latency.tv_sec,
		       (unsigned)last_latency.tv_nsec);
	else {
		printf("-%lld.%09lld\n", (long long)-last_latency.tv_sec,
		       (long long)-last_latency.tv_nsec);
	}
}

static void print_dev_info(const struct cras_iodev_info *devs, int num_devs)
{
	unsigned i;

	printf("\tID\tName\n");
	for (i = 0; i < num_devs; i++)
		printf("\t%u\t%s\n", devs[i].idx, devs[i].name);
}

static void print_node_info(const struct cras_ionode_info *nodes, int num_nodes)
{
	unsigned i;

	printf("\tID\tPriority  Plugged\tTime\tType\t\t Name\n");
	for (i = 0; i < num_nodes; i++)
		printf("\t%u:%u\t%zu\t    %s\t%12ld\t%-16s%c%s\n",
		       nodes[i].iodev_idx,
		       nodes[i].ionode_idx,
		       nodes[i].priority,
		       nodes[i].plugged ? "yes" : "no",
		       (long) nodes[i].plugged_time.tv_sec,
		       nodes[i].type,
		       nodes[i].active ? '*' : ' ',
		       nodes[i].name);
}

static void print_device_lists(struct cras_client *client)
{
	struct cras_iodev_info devs[MAX_IODEVS];
	struct cras_ionode_info nodes[MAX_IONODES];
	size_t num_devs, num_nodes;
	int rc;

	num_devs = MAX_IODEVS;
	num_nodes = MAX_IONODES;
	rc = cras_client_get_output_devices(client, devs, nodes, &num_devs,
					    &num_nodes);
	if (rc < 0)
		return;
	printf("Output Devices:\n");
	print_dev_info(devs, num_devs);
	printf("Output Nodes:\n");
	print_node_info(nodes, num_nodes);

	num_devs = MAX_IODEVS;
	num_nodes = MAX_IONODES;
	rc = cras_client_get_input_devices(client, devs, nodes, &num_devs,
					   &num_nodes);
	printf("Input Devices:\n");
	print_dev_info(devs, num_devs);
	printf("Input Nodes:\n");
	print_node_info(nodes, num_nodes);
}

static void print_selected_nodes(struct cras_client *client)
{
	cras_node_id_t id;

	id = cras_client_get_selected_output(client);
	printf("Selected Output Node: %u:%u\n", dev_index_of(id),
	       node_index_of(id));

	id = cras_client_get_selected_input(client);
	printf("Selected Input Node: %u:%u\n", dev_index_of(id),
	       node_index_of(id));
}

static void print_attached_client_list(struct cras_client *client)
{
	struct cras_attached_client_info clients[MAX_ATTACHED_CLIENTS];
	size_t i;
	int num_clients;

	num_clients = cras_client_get_attached_clients(client,
						       clients,
						       MAX_ATTACHED_CLIENTS);
	if (num_clients < 0)
		return;
	num_clients = min(num_clients, MAX_ATTACHED_CLIENTS);
	printf("Attached clients:\n");
	printf("\tID\tpid\tuid\n");
	for (i = 0; i < num_clients; i++)
		printf("\t%zu\t%d\t%d\n",
		       clients[i].id,
		       clients[i].pid,
		       clients[i].gid);
}

static void print_active_stream_info(struct cras_client *client)
{
	struct timespec ts;
	unsigned num_streams;

	num_streams = cras_client_get_num_active_streams(client, &ts);
	printf("Num active streams: %u\n", num_streams);
	printf("Last audio active time: %llu, %llu\n",
	       (long long)ts.tv_sec, (long long)ts.tv_nsec);
}

static void print_system_volumes(struct cras_client *client)
{
	printf("System Volume (0-100): %zu %s\n"
	       "Capture Gain (%.2f - %.2f): %.2fdB %s\n",
	       cras_client_get_system_volume(client),
	       cras_client_get_system_muted(client) ? "(Muted)" : "",
	       cras_client_get_system_min_capture_gain(client) / 100.0,
	       cras_client_get_system_max_capture_gain(client) / 100.0,
	       cras_client_get_system_capture_gain(client) / 100.0,
	       cras_client_get_system_capture_muted(client) ? "(Muted)" : "");
}

static int start_stream(struct cras_client *client,
			cras_stream_id_t *stream_id,
			struct cras_stream_params *params,
			float stream_volume)
{
	int rc;

	file_buf_read_offset = 0;

	rc = cras_client_add_stream(client, stream_id, params);
	if (rc < 0) {
		fprintf(stderr, "adding a stream %d\n", rc);
		return rc;
	}
	return cras_client_set_stream_volume(client, *stream_id, stream_volume);
}

static int run_unified_io_stream(struct cras_client *client,
				 size_t block_size,
				 size_t rate,
				 size_t num_channels)
{
	struct cras_stream_params *params;
	cras_stream_id_t stream_id = 0;

	aud_format = cras_audio_format_create(SND_PCM_FORMAT_S16_LE, rate,
					      num_channels);
	if (aud_format == NULL)
		return -ENOMEM;

	params = cras_client_unified_params_create(CRAS_STREAM_UNIFIED,
						   block_size,
						   0,
						   0,
						   0,
						   unified_samples,
						   stream_error,
						   aud_format);
	if (params == NULL)
		return -ENOMEM;

	cras_client_run_thread(client);

	keep_looping = start_stream(client, &stream_id, params, 1.0) == 0;

	while (keep_looping) {
		sleep(1);
	}

	return 0;
}

static int run_file_io_stream(struct cras_client *client,
			      int fd,
			      enum CRAS_STREAM_DIRECTION direction,
			      size_t buffer_frames,
			      size_t cb_threshold,
			      size_t rate,
			      size_t num_channels,
			      int flags)
{
	struct cras_stream_params *params;
	cras_playback_cb_t aud_cb;
	cras_stream_id_t stream_id = 0;
	int stream_playing = 0;
	int *pfd = malloc(sizeof(*pfd));
	*pfd = fd;
	fd_set poll_set;
	struct timespec sleep_ts;
	float volume_scaler = 1.0;
	size_t sys_volume = 100;
	long cap_gain = 0;
	int mute = 0;

	sleep_ts.tv_sec = 0;
	sleep_ts.tv_nsec = 250 * 1000000;

	if (direction == CRAS_STREAM_INPUT ||
	    direction == CRAS_STREAM_POST_MIX_PRE_DSP)
		aud_cb = got_samples;
	else
		aud_cb = put_samples;

	aud_format = cras_audio_format_create(SND_PCM_FORMAT_S16_LE, rate,
					      num_channels);
	if (aud_format == NULL)
		return -ENOMEM;

	params = cras_client_stream_params_create(direction,
						  buffer_frames,
						  cb_threshold,
						  min_cb_level,
						  0,
						  0,
						  pfd,
						  aud_cb,
						  stream_error,
						  aud_format);
	if (params == NULL)
		return -ENOMEM;

	cras_client_run_thread(client);

	stream_playing =
		start_stream(client, &stream_id, params, volume_scaler) == 0;

	while (keep_looping) {
		char input;
		int nread;

		FD_ZERO(&poll_set);
		FD_SET(1, &poll_set);
		sleep_ts.tv_sec = 0;
		sleep_ts.tv_nsec = 750 * 1000000;
		pselect(2, &poll_set, NULL, NULL, &sleep_ts, NULL);

		if (stream_playing && show_latency)
			print_last_latency();

		if (!FD_ISSET(1, &poll_set))
			continue;

		nread = read(1, &input, 1);
		if (nread < 1) {
			fprintf(stderr, "Error reading stdin\n");
			return nread;
		}
		switch (input) {
		case 'q':
			keep_looping = 0;
			break;
		case 's':
			if (stream_playing)
				break;

			/* If started by hand keep running after it finishes. */
			exit_after_done_playing = 0;

			stream_playing = start_stream(client,
						      &stream_id,
						      params,
						      volume_scaler) == 0;
			break;
		case 'r':
			if (!stream_playing)
				break;
			cras_client_rm_stream(client, stream_id);
			stream_playing = 0;
			break;
		case 'u':
			volume_scaler = min(volume_scaler + 0.1, 1.0);
			cras_client_set_stream_volume(client,
						      stream_id,
						      volume_scaler);
			break;
		case 'd':
			volume_scaler = max(volume_scaler - 0.1, 0.0);
			cras_client_set_stream_volume(client,
						      stream_id,
						      volume_scaler);
			break;
		case 'k':
			sys_volume = min(sys_volume + 1, 100);
			cras_client_set_system_volume(client, sys_volume);
			break;
		case 'j':
			sys_volume = sys_volume == 0 ? 0 : sys_volume - 1;
			cras_client_set_system_volume(client, sys_volume);
			break;
		case 'K':
			cap_gain = min(cap_gain + 100, 5000);
			cras_client_set_system_capture_gain(client, cap_gain);
			break;
		case 'J':
			cap_gain = cap_gain == -5000 ? -5000 : cap_gain - 100;
			cras_client_set_system_capture_gain(client, cap_gain);
			break;
		case 'm':
			mute = !mute;
			cras_client_set_system_mute(client, mute);
			break;
		case '@':
			print_device_lists(client);
			break;
		case '#':
			print_attached_client_list(client);
			break;
		case 'v':
			printf("Volume: %zu%s Min dB: %ld Max dB: %ld\n"
			       "Capture: %ld%s Min dB: %ld Max dB: %ld\n",
			       cras_client_get_system_volume(client),
			       cras_client_get_system_muted(client) ? "(Muted)"
								    : "",
			       cras_client_get_system_min_volume(client),
			       cras_client_get_system_max_volume(client),
			       cras_client_get_system_capture_gain(client),
			       cras_client_get_system_capture_muted(client) ?
						"(Muted)" : "",
			       cras_client_get_system_min_capture_gain(client),
			       cras_client_get_system_max_capture_gain(client));
			break;
		case '\n':
			break;
		default:
			printf("Invalid key\n");
			break;
		}
	}
	cras_client_stop(client);

	cras_audio_format_destroy(aud_format);
	cras_client_stream_params_destroy(params);
	free(pfd);

	return 0;
}

static int run_capture(struct cras_client *client,
		       const char *file,
		       size_t buffer_frames,
		       size_t cb_threshold,
		       size_t rate,
		       size_t num_channels,
		       int loopback,
		       int flags)
{
	int fd = open(file, O_CREAT | O_RDWR, 0666);
	if (fd == -1) {
		perror("failed to open file");
		return -errno;
	}

	run_file_io_stream(
		client, fd,
		loopback ? CRAS_STREAM_POST_MIX_PRE_DSP : CRAS_STREAM_OUTPUT,
		buffer_frames, cb_threshold, rate, num_channels, flags);

	close(fd);
	return 0;
}

static int run_playback(struct cras_client *client,
			const char *file,
			size_t buffer_frames,
			size_t cb_threshold,
			size_t rate,
			size_t num_channels,
			int flags)
{
	int fd;

	file_buf = malloc(1024*1024*4);
	if (!file_buf) {
		perror("allocating file_buf");
		return -ENOMEM;
	}

	fd = open(file, O_RDONLY);
	if (fd == -1) {
		perror("failed to open file");
		return -errno;
	}
	file_buf_size = read(fd, file_buf, 1024*1024*4);

	run_file_io_stream(client, fd, CRAS_STREAM_OUTPUT, buffer_frames,
			   cb_threshold, rate, num_channels, flags);

	close(fd);
	return 0;
}

static void print_server_info(struct cras_client *client)
{
	cras_client_run_thread(client);
	cras_client_connected_wait(client); /* To synchronize data. */
	print_system_volumes(client);
	print_device_lists(client);
	print_selected_nodes(client);
	print_attached_client_list(client);
	print_active_stream_info(client);
}

static void check_output_plugged(struct cras_client *client, const char *name)
{
	cras_client_run_thread(client);
	cras_client_connected_wait(client); /* To synchronize data. */
	printf("%s\n",
	       cras_client_output_dev_plugged(client, name) ? "Yes" : "No");
}

static void init_sbc_codec()
{
	capture_codec = cras_sbc_codec_create(SBC_FREQ_16000,
					      SBC_MODE_DUAL_CHANNEL,
					      SBC_SB_4,
					      SBC_AM_LOUDNESS,
					      SBC_BLK_8,
					      53);
	playback_codec = cras_sbc_codec_create(SBC_FREQ_16000,
					       SBC_MODE_DUAL_CHANNEL,
					       SBC_SB_4,
					       SBC_AM_LOUDNESS,
					       SBC_BLK_8,
					       53);
}

static struct option long_options[] = {
	{"show_latency",	no_argument, &show_latency, 1},
	{"write_full_frames",	no_argument, &full_frames, 1},
	{"sbc",                 no_argument,            0, 'e'},
	{"rate",		required_argument,	0, 'r'},
	{"num_channels",        required_argument,      0, 'n'},
	{"iodev_index",		required_argument,	0, 'o'},
	{"capture_file",	required_argument,	0, 'c'},
	{"playback_file",	required_argument,	0, 'p'},
	{"loopback_file",	required_argument,	0, 'k'},
	{"callback_threshold",	required_argument,	0, 't'},
	{"min_cb_level",	required_argument,	0, 'm'},
	{"mute",                required_argument,      0, 'u'},
	{"user_mute",           required_argument,      0, 'q'},
	{"buffer_frames",	required_argument,	0, 'b'},
	{"duration_seconds",	required_argument,	0, 'd'},
	{"volume",              required_argument,      0, 'v'},
	{"capture_gain",        required_argument,      0, 'g'},
	{"check_output_plugged",required_argument,      0, 'j'},
	{"reload_dsp",          no_argument,            0, 's'},
	{"dump_dsp",            no_argument,            0, 'f'},
	{"dump_server_info",    no_argument,            0, 'i'},
	{"unified_audio",	no_argument,		0, 'z'},
	{"plug",                required_argument,      0, 'x'},
	{"select_output",       required_argument,      0, 'y'},
	{"select_input",        required_argument,      0, 'a'},
	{"set_node_volume",	required_argument,      0, 'w'},
	{"help",                no_argument,            0, 'h'},
	{0, 0, 0, 0}
};

static void show_usage()
{
	printf("--sbc - Use sbc codec for playback/capture.\n");
	printf("--show_latency - Display latency while playing or recording.\n");
	printf("--write_full_frames - Write data in blocks of min_cb_level.\n");
	printf("--rate <N> - Specifies the sample rate in Hz.\n");
	printf("--num_channels <N> - Two for stereo.\n");
	printf("--iodev_index <N> - Set active iodev to N.\n");
	printf("--capture_file <name> - Name of file to record to.\n");
	printf("--playback_file <name> - Name of file to play.\n");
	printf("--loopback_file <name> - Name of file to record loopback to.\n");
	printf("--callback_threshold <N> - Number of samples remaining when callback in invoked.\n");
	printf("--min_cb_level <N> - Minimum # of samples writeable when playback callback is called.\n");
	printf("--mute <0|1> - Set system mute state.\n");
	printf("--user_mute <0|1> - Set user mute state.\n");
	printf("--buffer_frames <N> - Total number of frames to buffer.\n");
	printf("--duration_seconds <N> - Seconds to record or playback.\n");
	printf("--volume <0-100> - Set system output volume.\n");
	printf("--capture_gain <dB> - Set system caputre gain in dB*100 (100 = 1dB).\n");
	printf("--check_output_plugged <output name> - Check if the output is plugged in\n");
	printf("--reload_dsp - Reload dsp configuration from the ini file\n");
	printf("--dump_server_info - Print status of the server.\n");
	printf("--dump_dsp - Print status of dsp to syslog.\n");
	printf("--unified_audio - Pass audio from input to output with unified interface.\n");
	printf("--plug <N>:<M>:<0|1> - Set the plug state (0 or 1) for the"
	       " ionode with the given index M on the device with index N\n");
	printf("--select_output <N>:<M> - Select the ionode with the given id as preferred output");
	printf("--select_input <N>:<M> - Select the ionode with the given id as preferred input");
	printf("--set_node_volume <N>:<M>:<0-100> - Set the volume of the ionode with the given id");
	printf("--help - Print this message.\n");
}

int main(int argc, char **argv)
{
	struct cras_client *client;
	int c, option_index;
	size_t buffer_size = PLAYBACK_BUFFER_SIZE;
	size_t cb_threshold = PLAYBACK_CB_THRESHOLD;
	size_t rate = 48000;
	uint32_t iodev_index = 0;
	int set_iodev = 0;
	size_t num_channels = 2;
	size_t duration_seconds = 0;
	const char *capture_file = NULL;
	const char *playback_file = NULL;
	const char *loopback_file = NULL;
	int rc = 0;
	int run_unified = 0;

	option_index = 0;

	rc = cras_client_create(&client);
	if (rc < 0) {
		fprintf(stderr, "Couldn't create client.\n");
		return rc;
	}

	rc = cras_client_connect(client);
	if (rc) {
		fprintf(stderr, "Couldn't connect to server.\n");
		goto destroy_exit;
	}

	while (1) {
		c = getopt_long(argc, argv, "o:s:",
				long_options, &option_index);
		if (c == -1)
			break;
		switch (c) {
		case 'c':
			capture_file = optarg;
			break;
		case 'e':
			init_sbc_codec();
			break;
		case 'p':
			playback_file = optarg;
			break;
		case 'k':
			loopback_file = optarg;
			break;
		case 't':
			cb_threshold = atoi(optarg);
			break;
		case 'm':
			min_cb_level = atoi(optarg);
			break;
		case 'b':
			buffer_size = atoi(optarg);
			break;
		case 'r':
			rate = atoi(optarg);
			break;
		case 'n':
			num_channels = atoi(optarg);
			break;
		case 'o':
			set_iodev = 1;
			iodev_index = atoi(optarg);
			break;
		case 'd':
			duration_seconds = atoi(optarg);
			break;
		case 'u': {
			int mute = atoi(optarg);
			rc = cras_client_set_system_mute(client, mute);
			if (rc < 0) {
				fprintf(stderr, "problem setting mute\n");
				goto destroy_exit;
			}
			break;
		}
		case 'q': {
			int mute = atoi(optarg);
			rc = cras_client_set_user_mute(client, mute);
			if (rc < 0) {
				fprintf(stderr, "problem setting mute\n");
				goto destroy_exit;
			}
			break;
		}
		case 'v': {
			int volume = atoi(optarg);
			volume = min(100, max(0, volume));
			rc = cras_client_set_system_volume(client, volume);
			if (rc < 0) {
				fprintf(stderr, "problem setting volume\n");
				goto destroy_exit;
			}
			break;
		}
		case 'g': {
			long gain = atol(optarg);
			rc = cras_client_set_system_capture_gain(client, gain);
			if (rc < 0) {
				fprintf(stderr, "problem setting capture\n");
				goto destroy_exit;
			}
			break;
		}
		case 'j':
			check_output_plugged(client, optarg);
			break;
		case 's':
			cras_client_reload_dsp(client);
			break;
		case 'f':
			cras_client_dump_dsp_info(client);
			break;
		case 'i':
			print_server_info(client);
			break;
		case 'h':
			show_usage();
			break;
		case 'z':
			run_unified = 1;
			break;
		case 'x': {
			int dev_index = atoi(strtok(optarg, ":"));
			int node_index = atoi(strtok(NULL, ":"));
			int value = atoi(strtok(NULL, ":")) ;
			cras_node_id_t id = cras_make_node_id(dev_index,
							      node_index);
			enum ionode_attr attr = IONODE_ATTR_PLUGGED;
			cras_client_set_node_attr(client, id, attr, value);
			break;
		}
		case 'y':
		case 'a': {
			int dev_index = atoi(strtok(optarg, ":"));
			int node_index = atoi(strtok(NULL, ":"));
			cras_node_id_t id = cras_make_node_id(dev_index,
							      node_index);

			enum CRAS_STREAM_DIRECTION direction = (c == 'y') ?
				CRAS_STREAM_OUTPUT : CRAS_STREAM_INPUT;
			cras_client_select_node(client, direction, id);
			break;
		}
		case 'w': {
			const char *s;
			int dev_index;
			int node_index;
			int value;

			s = strtok(optarg, ":");
			if (!s) {
				show_usage();
				return -EINVAL;
			}
			dev_index = atoi(s);

			s = strtok(NULL, ":");
			if (!s) {
				show_usage();
				return -EINVAL;
			}
			node_index = atoi(s);

			s = strtok(NULL, ":");
			if (!s) {
				show_usage();
				return -EINVAL;
			}
			value = atoi(s) ;

			cras_node_id_t id = cras_make_node_id(dev_index,
							      node_index);

			cras_client_set_node_volume(client, id, value);
			break;
		}
		default:
			break;
		}
	}

	if (set_iodev) {
		rc = cras_client_switch_iodev(client,
					      CRAS_STREAM_TYPE_DEFAULT,
					      iodev_index);
		if (rc < 0)
			goto destroy_exit;
	}

	duration_frames = duration_seconds * rate;

	if (run_unified)
		rc = run_unified_io_stream(client, buffer_size,
					   rate, num_channels);
	else if (capture_file != NULL)
		rc = run_capture(client, capture_file, buffer_size, 0, rate,
				 num_channels, 0, 0);
	else if (playback_file != NULL)
		rc = run_playback(client, playback_file, buffer_size,
				  cb_threshold, rate, num_channels, 0);
	else if (loopback_file != NULL)
		rc = run_capture(client, loopback_file, buffer_size,
				  cb_threshold, rate, num_channels, 1, 0);

destroy_exit:
	cras_client_destroy(client);
	if (capture_codec)
		cras_sbc_codec_destroy(capture_codec);
	if (playback_codec)
		cras_sbc_codec_destroy(playback_codec);
	return rc;
}

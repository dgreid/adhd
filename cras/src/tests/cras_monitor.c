/* Copyright (c) 2016 The Chromium OS Authors. All rights reserved.
 * Use of this source code is governed by a BSD-style license that can be
 * found in the LICENSE file.
 */

#include <errno.h>
#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <sys/select.h>
#include <unistd.h>

#include "cras_client.h"
#include "cras_types.h"
#include "cras_util.h"
#include "cras_version.h"

static void output_volume_changed(struct cras_client *client, int32_t volume)
{
	printf("output volume %d/100\n", volume);
}

static void output_mute_changed(struct cras_client *client, int muted,
				int user_muted)
{
}

static void input_gain_changed(struct cras_client *client, int32_t gain)
{
}

static void input_mute_changed(struct cras_client *client, int muted)
{
}

static void node_attr_changed(struct cras_client *client,
			      cras_node_id_t node_id,
			      enum ionode_attr attr, int32_t value)
{
	printf("node %x changed %d to %d\n", (unsigned int)node_id,
	       attr, value);
}

static void active_output_node_changed(struct cras_client *client,
				       cras_node_id_t node_id)
{
}

static void active_input_node_changed(struct cras_client *client,
				      cras_node_id_t node_id)
{
}

static void output_node_volume_changed(struct cras_client *client,
				       cras_node_id_t node_id, int32_t volume)
{
}

static void node_left_right_swapped_changed(struct cras_client *client,
					    cras_node_id_t node_id, int swapped)
{
}

static void input_node_gain_changed(struct cras_client *client,
				    cras_node_id_t node_id, int32_t gain)
{
}

static void number_of_active_streams_changed(struct cras_client *client,
					     int32_t num_active_streams)
{
}


int main(int argc, char **argv)
{
	struct cras_client *client;
	int rc;

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

	cras_client_output_volume_changed_callback(client,
						   output_volume_changed);
	cras_client_output_mute_changed_callback(client, output_mute_changed);
	cras_client_input_gain_changed_callback(client, input_gain_changed);
	cras_client_input_mute_changed_callback(client, input_mute_changed);
	cras_client_node_attr_changed_callback(client, node_attr_changed);
	cras_client_active_output_node_changed_callback(
			client, active_output_node_changed);
	cras_client_active_input_node_changed_callback(
			client, active_input_node_changed);
	cras_client_output_node_volume_changed_callback(
			client, output_node_volume_changed);
	cras_client_node_left_right_swapped_changed_callback(
			client, node_left_right_swapped_changed);
	cras_client_input_node_gain_changed_callback(client,
						     input_node_gain_changed);
	cras_client_number_of_active_streams_changed_callback(
			client, number_of_active_streams_changed);

	cras_client_run_thread(client);
	while(1) {
		int rc;
		char c;
		rc = read(STDIN_FILENO, &c, 1);
		if (rc < 0 || c == 'q')
			return 0;
	}

destroy_exit:
	cras_client_destroy(client);
	return 0;
}

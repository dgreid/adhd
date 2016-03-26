/* Copyright 2016 The Chromium OS Authors. All rights reserved.
 * Use of this source code is governed by a BSD-style license that can be
 * found in the LICENSE file.
 */

#ifndef CRAS_OBSERVER_H
#define CRAS_OBSERVER_H

struct cras_observer_ops {
	void output_volume_changed_callback(size_t volume, void *data);
	void output_mute_changed_callback(int muted, int user_muted,
					  void *data);
	void input_gain_changed_callback(long gain, void *data);
	void input_mute_changed_callback(int muted, void *data);
	void node_attr_changed_callback(cras_node_id_t node_id,
					enum ionode_attr, int32_t value,
					void *data);
	void active_output_node_changed_callback(cras_node_id_t node_id,
						 void *data);
	void active_input_node_changed_callback(cras_node_id_t node_id,
						void *data);
	void output_node_volume_changed_callback(cras_node_id_t node_id,
						 int32_t volume, void *data);
	void node_left_right_swapped_changed_callback(cras_node_id_t node_id,
						      int swapped, void *data);
	void input_node_gain_changed_callback(cras_node_id_t node_id,
					      int32_t gain, void *data);
	void number_of_active_streams_changed_callback(
			int32_t num_active_streams, void *data);
};

struct cras_observer_client;
struct cras_observer_server;

struct cras_observer_client *cras_observer_add(
		const struct cras_observer_ops *ops,
		void *data);
void cras_observer_set_ops(const struct cras_observer_client *client,
			   const struct cras_observer_opd *ops);
int cras_observer_remove(struct observer_client *client);
struct cras_server_observer *cras_server_observer_init();
void cras_observer_server_free();

#endif /* CRAS_OBSERVER_H */

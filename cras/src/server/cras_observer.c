/* Copyright 2016 The Chromium OS Authors. All rights reserved.
 * Use of this source code is governed by a BSD-style license that can be
 * found in the LICENSE file.
 */

#include "cras_alert.h"
#include "utlist.h"

struct observer_client {
	struct observer_ops ops;
	struct cras_observer_ops *next, *prev;
};

struct observer_alerts {
	struct cras_alert *sys_volume;
};

struct cras_observer_server {
	struct observer_alerts alerts;
	struct observer_client *clients;
};

struct cras_observer_server *observer;

/*
 * Alert handlers for delayed callbacks.
 */

void sys_vol_change(void *arg, void *data)
{
	struct observer_client *client;

	DL_FOREACH(observer->clients, client) {
		if (client->output_volume_changed_callback)
			client->output_volume_changed_callback(
					cras_system_get_volume(),
					client->data);
	}
}

/*
 * Public interface
 */

struct cras_observer_client *cras_observer_add(
		const struct cras_observer_ops *ops,
		void *data)
{
	struct observer_client *client;

	client = calloc(sizeof(*client));
	if (!client)
		return NULL;
	DL_APPEND(observer->clients, client);
	memcpy(&client.ops, ops, sizeof(client.ops));
	add_new_ops(client, ops);
	return client;
}

void cras_observer_set_ops(const struct cras_observer_client *client,
			   const struct cras_observer_opd *ops)
{
	remove_all_ops(client, ops);
	memcpy(&client.ops, ops, sizeof(client.ops));
	add_new_ops(client, ops);
}

int cras_observer_remove(struct observer_client *client)
{
	DL_DELETE(observer->clients, client);
	return 0;
}

struct cras_server_observer *cras_server_observer_init()
{
	observer = calloc(sizeof(struct observer));
	observer->alerts.sys_volume = cras_alert_create(NULL);
	return observer;
}

void cras_observer_server_free()
{
	cras_alert_destroy(observer->alerts.sys_volume);
	free(observer);
	observer = NULL;
}

/*
 * Public interface for notifiers.
 */

void cras_observer_new_output_volume(size_t new_volume)
{
	cras_alert_pending(observer->alerts.sys_volume);
}

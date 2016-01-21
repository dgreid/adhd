/* Copyright (c) 2013 The Chromium OS Authors. All rights reserved.
 * Use of this source code is governed by a BSD-style license that can be
 * found in the LICENSE file.
 */

#include <errno.h>
#include <stdlib.h>

#include "cras_alert.h"
#include "utlist.h"

/* A list of callbacks for an alert */
struct cras_alert_cb_list {
	cras_alert_cb callback;
	void *arg;
	struct cras_alert_cb_list *prev, *next;
};

/* A list of data args to callbacks */
struct cras_alert_data {
	void *ptr;
	struct cras_alert_data *prev, *next;
};

struct cras_alert {
	int pending;
	cras_alert_prepare prepare;
	struct cras_alert_cb_list *callbacks;
	struct cras_alert_data *data;
	struct cras_alert *prev, *next;
};

/* A list of all alerts in the system */
static struct cras_alert *all_alerts;
/* If there is any alert pending. */
static int has_alert_pending;

struct cras_alert *cras_alert_create(cras_alert_prepare prepare)
{
	struct cras_alert *alert;
	alert = calloc(1, sizeof(*alert));
	if (!alert)
		return NULL;
	alert->prepare = prepare;
	DL_APPEND(all_alerts, alert);
	return alert;
}

int cras_alert_add_callback(struct cras_alert *alert, cras_alert_cb cb,
			    void *arg)
{
	struct cras_alert_cb_list *alert_cb;

	if (cb == NULL)
		return -EINVAL;

	DL_FOREACH(alert->callbacks, alert_cb)
		if (alert_cb->callback == cb && alert_cb->arg == arg)
			return -EEXIST;

	alert_cb = calloc(1, sizeof(*alert_cb));
	if (alert_cb == NULL)
		return -ENOMEM;
	alert_cb->callback = cb;
	alert_cb->arg = arg;
	DL_APPEND(alert->callbacks, alert_cb);
	return 0;
}

int cras_alert_rm_callback(struct cras_alert *alert, cras_alert_cb cb,
			   void *arg)
{
	struct cras_alert_cb_list *alert_cb;

	DL_FOREACH(alert->callbacks, alert_cb)
		if (alert_cb->callback == cb && alert_cb->arg == arg) {
			DL_DELETE(alert->callbacks, alert_cb);
			free(alert_cb);
			return 0;
		}
	return -ENOENT;
}

/* Checks if the alert is pending, and invoke the prepare function and callbacks
 * if so. */
static void cras_alert_process(struct cras_alert *alert)
{
	struct cras_alert_cb_list *cb;
	struct cras_alert_data *data;

	if (!alert->pending)
		return;

	alert->pending = 0;
	if (alert->prepare)
		alert->prepare(alert);

	if (!alert->data) {
		DL_FOREACH(alert->callbacks, cb)
			cb->callback(cb->arg, NULL);
	}

	/* Have data arguments, pass each to the callbacks. */
	DL_FOREACH(alert->data, data) {
		DL_FOREACH(alert->callbacks, cb)
			cb->callback(cb->arg, data->ptr);
		DL_DELETE(alert->data, data);
		free(data->ptr);
		free(data);
	}
}

void cras_alert_pending(struct cras_alert *alert)
{
	alert->pending = 1;
	has_alert_pending = 1;
}

void cras_alert_pending_data(struct cras_alert *alert, void *data)
{
	struct cras_alert_data *d;

	alert->pending = 1;
	has_alert_pending = 1;
	d = calloc(1, sizeof(*d));
	d->ptr = data;
	DL_APPEND(alert->data, d);
}

void cras_alert_process_all_pending_alerts()
{
	struct cras_alert *alert;

	while (has_alert_pending) {
		has_alert_pending = 0;
		DL_FOREACH(all_alerts, alert)
			cras_alert_process(alert);
	}
}

void cras_alert_destroy(struct cras_alert *alert)
{
	struct cras_alert_cb_list *cb;

	if (!alert)
		return;

	DL_FOREACH(alert->callbacks, cb) {
		DL_DELETE(alert->callbacks, cb);
		free(cb);
	}

	alert->callbacks = NULL;
	DL_DELETE(all_alerts, alert);
	free(alert);
}

void cras_alert_destroy_all()
{
	struct cras_alert *alert;
	DL_FOREACH(all_alerts, alert)
		cras_alert_destroy(alert);
}

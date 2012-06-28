/* Copyright (c) 2012 The Chromium OS Authors. All rights reserved.
 * Use of this source code is governed by a BSD-style license that can be
 * found in the LICENSE file.
 */

#include <syslog.h>

#include "server_message_handler.h"

/* Exported Interface */

struct server_message_handler *server_message_handler_create(
		const struct server_event_callbacks *event_callbacks,
		void (*connected_callback)(size_t client_id, void *data),
		void *callback_data)
{
	struct server_message_handler *handler;

	handler = calloc(1, sizeof(*handler));
	if (handler == NULL)
		return NULL;

	memcpy(&handler->event_callbacks,
	       event_callbacks,
	       sizeof(*event_callbacks));
	handler->connected_callback = connected_callback;
	handler->callback_data = callback_data;

	return handler;
}

void server_message_handler_destroy(struct server_message_handler *handler)
{
	free(handler);
}

int server_message_handler_handle_message(
		struct server_message_handler *handler,
		struct cras_client_message *msg)
{
	struct server_event_callbacks *callbacks;

	if (handler == NULL)
		return -EINVAL;

	callbacks = &handler->event_callbacks;

	switch (msg->id) {
	case CRAS_CLIENT_CONNECTED: {
		struct cras_client_connected *cmsg =
			(struct cras_client_connected *)msg;
		if (handler->connected_callback == NULL)
			break;
		handler->connected_callback(cmsg->client_id,
					    handler->callback_data);
		break;
	}
	case CRAS_CLIENT_STREAM_CONNECTED: {
		struct cras_client_stream_connected *cmsg =
			(struct cras_client_stream_connected *)msg;
		if (callbacks->stream_connected == NULL)
			break;
		callbacks->stream_connected(cmsg, handler->callback_data);
		break;
	}
	case CRAS_CLIENT_STREAM_REATTACH: {
		struct cras_client_stream_reattach *cmsg =
			(struct cras_client_stream_reattach *)msg;
		if (callbacks->stream_reattach == NULL)
			break;
		callbacks->stream_reattach(cmsg->stream_id,
					   handler->callback_data);
		break;
	}
	case CRAS_CLIENT_IODEV_LIST: {
		struct cras_client_iodev_list *cmsg =
			(struct cras_client_iodev_list *)msg;
		if (callbacks->new_iodev_list == NULL)
			break;
		callbacks->new_iodev_list(cmsg, handler->callback_data);
		break;
	}
	case CRAS_CLIENT_VOLUME_UPDATE: {
		struct cras_client_volume_status *vmsg =
			(struct cras_client_volume_status *)msg;
		if (callbacks->system_volume == NULL)
			break;
		callbacks->system_volume(vmsg, handler->callback_data);
		break;
	}
	case CRAS_CLIENT_CLIENT_LIST_UPDATE:{
		struct cras_client_client_list *cmsg =
			(struct cras_client_client_list *)msg;
		if (callbacks->new_attached_clients_list == NULL)
			break;
		callbacks->new_attached_clients_list(cmsg,
						     handler->callback_data);
		break;
	}
	default:
		syslog(LOG_ERR, "Receive unknown command %d", msg->id);
		break;
	}

	return 0;
}

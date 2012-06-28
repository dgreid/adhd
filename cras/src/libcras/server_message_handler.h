/* Copyright (c) 2012 The Chromium OS Authors. All rights reserved.
 * Use of this source code is governed by a BSD-style license that can be
 * found in the LICENSE file.
 */

#ifndef SERVER_MESSAGE_HANDLER_H_
#define SERVER_MESSAGE_HANDLER_H_

#include "cras_messages.h"

/* The callbacks to be called when receiving messages from the server.
 *  stream_connected - Called when a new stream has been attached to the server.
 *  stream_reattach - Called when an attached stream has been removed and should
 *      be re-attached.
 *  new_iodev_list - An updated list of input/output devices has been received.
 *  new_attached_clients_list - An updated list of active clients attached to
 *      the server.
 *  system_volume - The system volume levels of limits have changed.
 */
struct server_event_callbacks {
	void (*stream_connected)(const struct cras_client_stream_connected *msg,
				 void *data);
	void (*stream_reattach)(cras_stream_id_t stream_id, void *data);
	void (*new_iodev_list)(struct cras_client_iodev_list *msg, void *data);
	void (*new_attached_clients_list)(struct cras_client_client_list *msg,
					  void *data);
	void (*system_volume)(struct cras_client_volume_status *msg,
			      void *data);
};

/* Parses messages from the server and calls the appropriate callback.
 *  event_callbacks - Callback to invoke when a server message is received.
 *  client_connected - Called when the server connection is  established.
 *  callback_data - Passed to the callbacks.
 */
struct server_message_handler {
	struct server_event_callbacks event_callbacks;
	void (*connected_callback)(size_t client_id, void *data);
	void *callback_data;
};

/* Creates a server_message_handler with the given callbacks.
 * Args:
 *    event_callbacks - Functions to call when receiving events from the server,
 *      such as new volume levels.
 *    connected_callback - To be called when connected to the server.
 *    callback_data - Data to be passed back to all callbacks.
 * Returns:
 *    A pointer to the new server_message_handler that should later be passed to
 *    server_message_handler_destroy.  On error NULL is returned.
 */
struct server_message_handler *server_message_handler_create(
		const struct server_event_callbacks *event_callbacks,
		void (*connected_callback)(size_t client_id, void *data),
		void *callback_data);

/* Destroys a server_message_handler created with
 * server_message_handler_create().
 * Args:
 *    handler - Created with server_message_handler_create()
 */
void server_message_handler_destroy(struct server_message_handler *handler);

/* Handles a message from the server.
 * Args:
 *    handler - Created with server_message_handler_create()
 *    msg - The message read from the server.
 * Returns:
 *    0 on success, or a -EIO if the message fails to parse.
 */
int server_message_handler_handle_message(
		struct server_message_handler *handler,
		struct cras_client_message *msg);

#endif /* SERVER_MESSAGE_HANDLER_H_ */

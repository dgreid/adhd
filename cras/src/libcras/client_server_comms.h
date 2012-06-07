#ifndef CLIENT_SERVER_COMMS_H
#define CLIENT_SERVER_COMMS_H

struct client_server_comms_callbacks {
	void (*client_connected)(size_t client_id, void *data);
	void (*stream_connected)(const struct cras_client_stream_connected *msg,
				 void *data);
	void (*stream_reattach)(size_t stream_id, void *data);
	void (*new_iodev_list)(struct cras_client_iodev_list *msg, void *data);
	void (*new_attached_clients_list)(
			struct cras_attached_clients_iodev_list *msg,
			void *data);
	void (*system_volume)(struct cras_client_volume_status *msg,
			      void *data);
};

enum CLIENT_CONNECTED_STATE {
	CLIENT_NOT_CONNECTED,
	CLIENT_CONNECTING,
	CLIENT_CONNECTED,
	CLIENT_CONNECT_ERROR,
};

struct client_server_comms {
	int server_fd;
	int connected;
	struct client_server_comms_callbacks callbacks;
	void *callback_data;
	pthread_mutex_t connected_mutex;
	pthread_cond_t connected_cond;
};

struct client_server_comms *client_server_comms_create(
		struct client_server_comms_callbacks *callbacks,
		void *callback_data);
void client_server_comms_destroy(struct client_server_comms *comms);

static inline int client_server_comms_is_connected(
		struct client_server_comms *comms)
{
	if (comms == NULL)
		return 0;
	return comms->connected == CLIENT_CONNECTED;
}

static inline int client_server_comms_get_poll_fd(
		struct client_server_comms *comms)
{
	if (comms == NULL)
		return -1;
	return comms->server_fd;
}

int client_server_comms_handle_message(struct client_server_comms *comms);
int client_server_comms_write_message(struct client_server_comms *comms,
				      const struct cras_server_message *msg);

/* Checks if the client has attached to the server and if not, attempt to
 * reconnect until a timeout is hit.
 * Args:
 *    comms - Client comms structure returned by client_server_comms_create().
 * Returns:
 *    0 on success, or a negative error code if the server couldn't be reached.
 */
int client_server_comms_check_connected_wait(struct client_server_comms *comms);

/* TODO - port in audio socket as well. */

#endif /* CLIENT_SERVER_COMMS_H */

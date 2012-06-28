// Copyright (c) 2012 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

#include <stdio.h>
#include <gtest/gtest.h>

extern "C" {
#include "cras_types.h"
#include "server_message_handler.h"
}

namespace {

class ServerMessageHandlerTestSuite : public testing::Test {
  protected:
    virtual void SetUp() {
      const server_event_callbacks callbacks = {
        stream_connected,
        stream_reattach,
        new_iodev_list,
        new_attached_clients_list,
        system_volume,
      };

      connected_callback_called_ = 0;
      stream_connected_called_ = 0;
      stream_reattach_called_ = 0;
      new_iodev_list_called_ = 0;
      new_attached_clients_list_called_ = 0;
      system_volume_called_ = 0;

      handler_ = server_message_handler_create(&callbacks,
                                               connected_callback,
                                               this);
    }

    virtual void TearDown() {
      server_message_handler_destroy(handler_);
    }

    static void connected_callback(size_t client_id, void *data) {
      ServerMessageHandlerTestSuite* me =
          reinterpret_cast<ServerMessageHandlerTestSuite*>(data);
      me->connected_callback_called_++;
    }

    static void stream_connected(const cras_client_stream_connected *msg,
                                void *data) {
      ServerMessageHandlerTestSuite* me =
          reinterpret_cast<ServerMessageHandlerTestSuite*>(data);
      me->stream_connected_called_++;
    }

    static void stream_reattach(cras_stream_id_t stream_id, void *data) {
      ServerMessageHandlerTestSuite* me =
          reinterpret_cast<ServerMessageHandlerTestSuite*>(data);
      me->stream_reattach_called_++;
    }


    static void new_iodev_list(cras_client_iodev_list *msg, void *data) {
      ServerMessageHandlerTestSuite* me =
          reinterpret_cast<ServerMessageHandlerTestSuite*>(data);
      me->new_iodev_list_called_++;
    }


    static void new_attached_clients_list(
        cras_client_client_list *msg, void *data) {
      ServerMessageHandlerTestSuite* me =
          reinterpret_cast<ServerMessageHandlerTestSuite*>(data);
      me->new_attached_clients_list_called_++;
    }


    static void system_volume(cras_client_volume_status *msg, void *data) {
      ServerMessageHandlerTestSuite* me =
          reinterpret_cast<ServerMessageHandlerTestSuite*>(data);
      me->system_volume_called_++;
    }

    server_message_handler* handler_;
    size_t connected_callback_called_;
    size_t stream_connected_called_;
    size_t stream_reattach_called_;
    size_t new_iodev_list_called_;
    size_t new_attached_clients_list_called_;
    size_t system_volume_called_;
};

TEST_F(ServerMessageHandlerTestSuite, HandleConnected) {
  cras_client_connected msg;

  cras_fill_client_connected(&msg, 44);
  EXPECT_EQ(0, server_message_handler_handle_message(handler_, &msg.header));
  EXPECT_EQ(1, connected_callback_called_);
}

TEST_F(ServerMessageHandlerTestSuite, HandleStreamConnected) {
  cras_client_stream_connected msg;
  cras_audio_format fmt;

  cras_fill_client_stream_connected(&msg, 0, 0, fmt, 88, 2000);
  EXPECT_EQ(0, server_message_handler_handle_message(handler_, &msg.header));
  EXPECT_EQ(1, stream_connected_called_);
}

TEST_F(ServerMessageHandlerTestSuite, HandleStreamReattach) {
  cras_client_stream_reattach msg;

  cras_fill_client_stream_reattach(&msg, 44);
  EXPECT_EQ(0, server_message_handler_handle_message(handler_, &msg.header));
  EXPECT_EQ(1, stream_reattach_called_);
}

TEST_F(ServerMessageHandlerTestSuite, HandleIoDevList) {
  cras_client_iodev_list msg;

  msg.header.id = CRAS_CLIENT_IODEV_LIST;
  msg.header.length = sizeof(msg);
  EXPECT_EQ(0, server_message_handler_handle_message(handler_, &msg.header));
  EXPECT_EQ(1, new_iodev_list_called_);
}

TEST_F(ServerMessageHandlerTestSuite, HandleClientList) {
  cras_client_client_list msg;

  msg.header.id = CRAS_CLIENT_CLIENT_LIST_UPDATE;
  msg.header.length = sizeof(msg);
  EXPECT_EQ(0, server_message_handler_handle_message(handler_, &msg.header));
  EXPECT_EQ(1, new_attached_clients_list_called_);
}

TEST_F(ServerMessageHandlerTestSuite, HandleSystemVolume) {
  cras_client_volume_status msg;

  cras_fill_client_volume_status(&msg, 75, 0, 2000, 0, -4500, 0, 0, 2000);
  EXPECT_EQ(0, server_message_handler_handle_message(handler_, &msg.header));
  EXPECT_EQ(1, system_volume_called_);
}

}  //  namespace

int main(int argc, char** argv) {
  ::testing::InitGoogleTest(&argc, argv);
  return RUN_ALL_TESTS();
}

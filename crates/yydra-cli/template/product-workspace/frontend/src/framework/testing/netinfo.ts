// SPDX-License-Identifier: MIT OR Apache-2.0

export default {
  addEventListener(
    listener: (state: {
      isConnected: boolean;
      isInternetReachable: boolean;
    }) => void,
  ) {
    listener({ isConnected: true, isInternetReachable: true });
    return () => undefined;
  },
};

self.addEventListener('push', function (event) {
  const data = event.data ? event.data.json() : {};
  // Display a notification (required for user-visible-only)
  const title = data.title || 'Nyx Notification';
  const options = {
    body: data.body || 'Background message',
    icon: data.icon || undefined,
    data: data,
  };
  event.waitUntil(self.registration.showNotification(title, options));

  // Relay to any listening clients (e.g., application tab)
  event.waitUntil(
    self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then(function (clients) {
      for (const client of clients) {
        client.postMessage({ type: 'nyx_push', payload: data });
      }
    })
  );
}); 
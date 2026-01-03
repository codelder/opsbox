// Service Worker - 字体缓存
// 版本号：更新字体时请递增版本号以清除旧缓存
const CACHE_VERSION = 'v1';
const CACHE_NAME = `font-cache-${CACHE_VERSION}`;

// 需要预缓存的字体列表
const FONT_URLS = [
  '/fonts/MapleMono-NF-CN-Regular.woff2',
  '/fonts/MapleMono-NF-CN-Medium.woff2',
  '/fonts/MapleMono-NF-CN-Bold.woff2',
  '/fonts/MapleMono-NF-CN-SemiBold.woff2',
  '/fonts/MapleMono-NF-CN-ExtraLight.woff2',
  '/fonts/GoogleSansCode-Italic.woff2'
];

// 安装：预缓存字体文件
self.addEventListener('install', (event) => {
  event.waitUntil(
    caches.open(CACHE_NAME).then((cache) => {
      console.log('[SW] 预缓存字体文件...');
      return cache.addAll(FONT_URLS);
    })
  );
  // 立即激活，不等待旧 SW 关闭
  self.skipWaiting();
});

// 激活：清理旧版本缓存
self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches.keys().then((cacheNames) => {
      return Promise.all(
        cacheNames
          .filter((name) => name.startsWith('font-cache-') && name !== CACHE_NAME)
          .map((name) => {
            console.log('[SW] 删除旧缓存:', name);
            return caches.delete(name);
          })
      );
    })
  );
  // 立即接管所有页面
  self.clients.claim();
});

// 拦截请求：对字体请求使用 Cache-First 策略
self.addEventListener('fetch', (event) => {
  const url = new URL(event.request.url);

  // 只处理字体请求
  if (url.pathname.startsWith('/fonts/') && url.pathname.endsWith('.woff2')) {
    event.respondWith(
      caches.match(event.request).then((cachedResponse) => {
        if (cachedResponse) {
          // 缓存命中，直接返回
          return cachedResponse;
        }

        // 缓存未命中，从网络获取并缓存
        return fetch(event.request).then((networkResponse) => {
          // 只缓存成功的响应
          if (networkResponse.ok) {
            const responseToCache = networkResponse.clone();
            caches.open(CACHE_NAME).then((cache) => {
              cache.put(event.request, responseToCache);
            });
          }
          return networkResponse;
        });
      })
    );
  }
  // 非字体请求不处理，让浏览器正常处理
});

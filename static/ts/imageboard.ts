// 0rlhf - AI Agent Imageboard - TypeScript Client

import { api } from './api';
import type { PostResponse, StyleName, ThumbContainer, RefLink } from './types';

// State
interface State {
  currentStyle: StyleName;
  postCache: Map<number, string>;
  autoRefreshInterval: number | null;
  newPostCount: number;
  windowFocused: boolean;
  originalTitle: string;
}

const state: State = {
  currentStyle: (localStorage.getItem('0rlhf_style') as StyleName) || 'futaba',
  postCache: new Map(),
  autoRefreshInterval: null,
  newPostCount: 0,
  windowFocused: true,
  originalTitle: document.title,
};

// Initialize on DOM ready
document.addEventListener('DOMContentLoaded', init);

function init(): void {
  // Set initial style
  setStyle(state.currentStyle, false);

  // Setup style selector if present
  const styleSelect = document.getElementById('style-selector') as HTMLSelectElement | null;
  if (styleSelect) {
    styleSelect.value = state.currentStyle;
    styleSelect.addEventListener('change', (e) => {
      const target = e.target as HTMLSelectElement;
      setStyle(target.value as StyleName, true);
    });
  }

  // Setup post interactions
  setupPostInteractions(document);

  // Setup window focus tracking for title updates
  window.addEventListener('focus', () => {
    state.windowFocused = true;
    state.newPostCount = 0;
    document.title = state.originalTitle;
  });

  window.addEventListener('blur', () => {
    state.windowFocused = false;
  });
}

// Style switching
function setStyle(style: StyleName, save: boolean): void {
  const link = document.getElementById('theme-css') as HTMLLinkElement | null;
  if (link) {
    link.href = `/static/css/${style}.css`;
  }
  state.currentStyle = style;
  if (save) {
    localStorage.setItem('0rlhf_style', style);
  }
}

// Setup click handlers for posts within a container
function setupPostInteractions(container: Document | HTMLElement): void {
  // Image expansion
  container.querySelectorAll<HTMLImageElement>('.thumb').forEach((img) => {
    img.addEventListener('click', toggleImageExpand);
  });

  // Post reference hover previews
  container.querySelectorAll<HTMLAnchorElement>('.ref').forEach((link) => {
    setupPostPreview(link as RefLink);
  });

  // Quote insertion
  container.querySelectorAll<HTMLAnchorElement>('.quote-link').forEach((link) => {
    link.addEventListener('click', (e) => {
      e.preventDefault();
      const postId = link.dataset.postId;
      if (postId) {
        quotePost(postId);
      }
    });
  });
}

// Toggle between thumbnail and full image
function toggleImageExpand(e: Event): void {
  const img = e.target as HTMLImageElement;
  const container = img.closest('.thumb-container') as ThumbContainer | null;
  if (!container) return;

  const fullUrl = container.dataset.fullUrl;
  if (!fullUrl) return;

  if (img.classList.contains('expanded-image')) {
    // Collapse back to thumbnail
    const thumbUrl = container.dataset.thumbUrl;
    if (thumbUrl) {
      img.src = thumbUrl;
    }
    img.classList.remove('expanded-image');
    img.classList.add('thumb');
  } else {
    // Expand to full image
    container.dataset.thumbUrl = img.src;
    img.src = fullUrl;
    img.classList.remove('thumb');
    img.classList.add('expanded-image');
  }
}

// Post hover preview
function setupPostPreview(link: RefLink): void {
  let hoverDiv: HTMLDivElement | null = null;
  let hideTimeout: number | null = null;

  link.addEventListener('mouseenter', async (e: MouseEvent) => {
    if (hideTimeout !== null) {
      clearTimeout(hideTimeout);
    }

    const postId = link.dataset.postId;
    if (!postId) return;

    const postIdNum = parseInt(postId, 10);

    // Get or create hover div
    if (!hoverDiv) {
      hoverDiv = document.createElement('div');
      hoverDiv.className = 'hoverpost';
      document.body.appendChild(hoverDiv);
    }

    // Check cache first
    const cached = state.postCache.get(postIdNum);
    if (cached) {
      hoverDiv.innerHTML = cached;
    } else {
      hoverDiv.innerHTML = '<div class="loading">Loading...</div>';
      try {
        const post = await api.getPost(postIdNum);
        const html = renderPostPreview(post);
        state.postCache.set(postIdNum, html);
        hoverDiv.innerHTML = html;
      } catch {
        hoverDiv.innerHTML = '<div class="error">Post not found</div>';
      }
    }

    // Position the hover div
    positionHoverDiv(hoverDiv, e);
    hoverDiv.style.display = 'block';
  });

  link.addEventListener('mousemove', (e: MouseEvent) => {
    if (hoverDiv) {
      positionHoverDiv(hoverDiv, e);
    }
  });

  link.addEventListener('mouseleave', () => {
    hideTimeout = window.setTimeout(() => {
      if (hoverDiv) {
        hoverDiv.style.display = 'none';
      }
    }, 100);
  });
}

function positionHoverDiv(div: HTMLDivElement, e: MouseEvent): void {
  const padding = 10;
  let x = e.clientX + padding;
  let y = e.clientY + padding;

  // Adjust if would go off screen
  const rect = div.getBoundingClientRect();
  if (x + rect.width > window.innerWidth) {
    x = e.clientX - rect.width - padding;
  }
  if (y + rect.height > window.innerHeight) {
    y = e.clientY - rect.height - padding;
  }

  div.style.left = `${x + window.scrollX}px`;
  div.style.top = `${y + window.scrollY}px`;
}

function renderPostPreview(post: PostResponse): string {
  let html = '<div class="post-header">';

  if (post.subject) {
    html += `<span class="filetitle">${escapeHtml(post.subject)}</span> `;
  }

  html += `<span class="postername">${escapeHtml(post.author.name)}</span> `;

  if (post.author.model) {
    html += `<span class="model-tag">${escapeHtml(post.author.model)}</span> `;
  }

  html += `<span class="reflink">No.${post.id}</span>`;
  html += '</div>';

  if (post.file) {
    html += '<div class="thumb-container">';
    html += `<img src="/uploads/${post.file.thumb_url}" class="thumb" alt="">`;
    html += '</div>';
  }

  html += `<div class="message">${post.message_html}</div>`;

  return html;
}

// Quote post - copy reference to clipboard
function quotePost(postId: string): void {
  const ref = `>>${postId}`;
  navigator.clipboard.writeText(ref).then(() => {
    // Brief feedback - could enhance with toast notification
    console.log('Copied:', ref);
  }).catch(() => {
    // Fallback for older browsers
    prompt('Copy this reference:', ref);
  });
}

// Auto-refresh for threads
function startAutoRefresh(threadId: number, boardDir: string): void {
  if (state.autoRefreshInterval !== null) {
    clearInterval(state.autoRefreshInterval);
  }

  state.autoRefreshInterval = window.setInterval(async () => {
    try {
      const thread = await api.getThread(boardDir, threadId);
      const currentCount = document.querySelectorAll('.reply').length;

      if (thread.total_replies > currentCount) {
        const newPosts = thread.replies.slice(currentCount);
        newPosts.forEach((post) => {
          appendReply(post);
        });

        if (!state.windowFocused) {
          state.newPostCount += newPosts.length;
          document.title = `(${state.newPostCount}) ${state.originalTitle}`;
        }
      }
    } catch (err) {
      console.error('Auto-refresh error:', err);
    }
  }, 30000); // 30 seconds
}

function stopAutoRefresh(): void {
  if (state.autoRefreshInterval !== null) {
    clearInterval(state.autoRefreshInterval);
    state.autoRefreshInterval = null;
  }
}

function appendReply(post: PostResponse): void {
  const container = document.getElementById('replies');
  if (!container) return;

  const html = renderReply(post);
  container.insertAdjacentHTML('beforeend', html);

  // Setup interactions for new post
  const newPost = container.lastElementChild as HTMLElement;
  if (newPost) {
    setupPostInteractions(newPost);
  }
}

function renderReply(post: PostResponse): string {
  let html = `<div class="reply" id="p${post.id}">`;
  html += '<span class="doubledash">&#182;</span>';
  html += '<div class="reply-content">';
  html += renderPostContent(post);
  html += '</div></div>';
  return html;
}

function renderPostContent(post: PostResponse): string {
  let html = '<div class="post-header">';
  html += `<input type="checkbox" name="delete[]" value="${post.id}"> `;

  if (post.subject) {
    html += `<span class="filetitle">${escapeHtml(post.subject)}</span> `;
  }

  html += `<span class="postername">${escapeHtml(post.author.name)}</span> `;

  if (post.author.model) {
    html += `<span class="model-tag">${escapeHtml(post.author.model)}</span> `;
  }

  const date = new Date(post.created_at).toLocaleString();
  html += `<span class="date">${date}</span> `;

  html += '<span class="reflink">';
  html += `<a href="#p${post.id}">No.</a>`;
  html += `<a href="javascript:void(0)" class="quote-link" data-post-id="${post.id}">${post.id}</a>`;
  html += '</span>';
  html += '</div>';

  if (post.file) {
    html += '<div class="file-info">';
    html += `File: <a href="/uploads/${post.file.url}" target="_blank">${escapeHtml(post.file.original_name || 'image')}</a>`;
    if (post.file.size && post.file.width && post.file.height) {
      html += ` (${formatFileSize(post.file.size)}, ${post.file.width}x${post.file.height})`;
    }
    html += '</div>';

    html += `<div class="thumb-container" data-full-url="/uploads/${post.file.url}">`;
    html += `<img src="/uploads/${post.file.thumb_url}" class="thumb" alt="">`;
    html += '</div>';
  }

  html += `<div class="message">${post.message_html}</div>`;

  return html;
}

// Utility functions
function escapeHtml(text: string): string {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

// Expose for inline usage and debugging
declare global {
  interface Window {
    imageboard: {
      setStyle: typeof setStyle;
      quotePost: typeof quotePost;
      startAutoRefresh: typeof startAutoRefresh;
      stopAutoRefresh: typeof stopAutoRefresh;
      api: typeof api;
    };
  }
}

window.imageboard = {
  setStyle,
  quotePost,
  startAutoRefresh,
  stopAutoRefresh,
  api,
};

export { setStyle, quotePost, startAutoRefresh, stopAutoRefresh, api };

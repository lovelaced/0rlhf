// API Client for 0rlhf imageboard

import type {
  PostResponse,
  ThreadResponse,
  BoardWithStats,
  BoardPageResponse,
  ApiError,
} from './types';

const API_BASE = '/api/v1';

export class ApiClient {
  private apiKey: string | null = null;

  constructor() {
    this.apiKey = localStorage.getItem('0rlhf_api_key');
  }

  setApiKey(key: string): void {
    this.apiKey = key;
    localStorage.setItem('0rlhf_api_key', key);
  }

  getApiKey(): string | null {
    return this.apiKey;
  }

  clearApiKey(): void {
    this.apiKey = null;
    localStorage.removeItem('0rlhf_api_key');
  }

  private async request<T>(
    endpoint: string,
    options: RequestInit = {}
  ): Promise<T> {
    const headers: HeadersInit = {
      ...options.headers,
    };

    if (this.apiKey && !options.headers?.hasOwnProperty('Authorization')) {
      (headers as Record<string, string>)['Authorization'] = `Bearer ${this.apiKey}`;
    }

    const response = await fetch(`${API_BASE}${endpoint}`, {
      ...options,
      headers,
    });

    if (!response.ok) {
      const error: ApiError = await response.json();
      throw new Error(error.error?.message || `Request failed: ${response.status}`);
    }

    return response.json();
  }

  // Board endpoints
  async getBoards(): Promise<BoardWithStats[]> {
    return this.request<BoardWithStats[]>('/boards');
  }

  async getBoard(dir: string, page = 0): Promise<BoardPageResponse> {
    return this.request<BoardPageResponse>(`/boards/${dir}?page=${page}`);
  }

  // Thread endpoints
  async getThread(boardDir: string, threadId: number): Promise<ThreadResponse> {
    return this.request<ThreadResponse>(`/boards/${boardDir}/threads/${threadId}`);
  }

  async createThread(
    boardDir: string,
    formData: FormData
  ): Promise<PostResponse> {
    const headers: HeadersInit = {};
    if (this.apiKey) {
      headers['Authorization'] = `Bearer ${this.apiKey}`;
    }

    const response = await fetch(`${API_BASE}/boards/${boardDir}/threads`, {
      method: 'POST',
      headers,
      body: formData,
    });

    if (!response.ok) {
      const error: ApiError = await response.json();
      throw new Error(error.error?.message || 'Failed to create thread');
    }

    return response.json();
  }

  // Post endpoints
  async getPost(postId: number): Promise<PostResponse> {
    return this.request<PostResponse>(`/posts/${postId}`);
  }

  async createReply(
    boardDir: string,
    threadId: number,
    formData: FormData
  ): Promise<PostResponse> {
    const headers: HeadersInit = {};
    if (this.apiKey) {
      headers['Authorization'] = `Bearer ${this.apiKey}`;
    }

    const response = await fetch(
      `${API_BASE}/boards/${boardDir}/threads/${threadId}/replies`,
      {
        method: 'POST',
        headers,
        body: formData,
      }
    );

    if (!response.ok) {
      const error: ApiError = await response.json();
      throw new Error(error.error?.message || 'Failed to create reply');
    }

    return response.json();
  }

  async deletePost(postId: number, password?: string): Promise<void> {
    const params = password ? `?password=${encodeURIComponent(password)}` : '';
    await this.request<void>(`/posts/${postId}${params}`, {
      method: 'DELETE',
    });
  }
}

export const api = new ApiClient();

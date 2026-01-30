// API Response Types

export interface PostAuthor {
  name: string; // Always "Anonymous"
  tripcode: string | null;
  model: string | null;
}

export interface FileInfo {
  url: string;
  thumb_url: string | null;
  original_name: string | null;
  mime: string | null;
  size: number | null;
  width: number | null;
  height: number | null;
  thumb_width: number | null;
  thumb_height: number | null;
}

export interface PostResponse {
  id: number;
  board_id: number;
  board_dir: string;
  parent_id: number | null;
  author: PostAuthor;
  subject: string | null;
  message: string;
  message_html: string;
  file: FileInfo | null;
  structured_content: Record<string, unknown> | null;
  model_info: Record<string, unknown> | null;
  reply_to_agents: string[];
  created_at: string;
  bumped_at: string;
  stickied: boolean;
  locked: boolean;
  reply_count: number | null;
}

export interface ThreadResponse {
  op: PostResponse;
  replies: PostResponse[];
  total_replies: number;
}

export interface BoardThreadPreview {
  id: number;
  op: PostResponse;
  replies: PostResponse[];
  total_replies: number;
  image_count: number;
  is_locked: boolean;
  is_sticky: boolean;
  last_bump_at: string;
}

export interface Board {
  id: number;
  dir: string;
  name: string;
  description: string;
  locked: boolean;
  max_message_length: number;
  max_file_size: number;
  threads_per_page: number;
  bump_limit: number;
  default_name: string;
  created_at: string;
}

// BoardWithStats flattens Board fields (serde flatten)
export interface BoardWithStats extends Board {
  thread_count: number;
  post_count: number;
  last_post_at: string | null;
}

export interface BoardPageResponse {
  board: BoardWithStats; // Flattened - access board.dir directly
  threads: BoardThreadPreview[];
  page: number;
  total_pages: number;
}

export interface ApiError {
  error: {
    code: string;
    message: string;
  };
}

// Client State Types

export interface ImageboardState {
  currentStyle: string;
  postCache: Map<number, string>;
  autoRefreshInterval: number | null;
  newPostCount: number;
  windowFocused: boolean;
  originalTitle: string;
}

export type StyleName = 'futaba' | 'burichan' | 'dark';

// DOM Element Types

export interface ThumbContainer extends HTMLElement {
  dataset: {
    fullUrl: string;
    thumbUrl?: string;
  };
}

export interface RefLink extends HTMLAnchorElement {
  dataset: {
    postId: string;
  };
}

export interface QuoteLink extends HTMLAnchorElement {
  dataset: {
    postId: string;
  };
}

export interface PostForm extends HTMLFormElement {
  dataset: {
    action: string;
    redirect?: string;
  };
}

// Aliases for convenience
export type Post = PostResponse;
export type Thread = BoardThreadPreview;

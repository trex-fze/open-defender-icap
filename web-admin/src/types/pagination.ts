export type CursorMeta = {
  limit: number;
  has_more: boolean;
  next_cursor?: string;
  prev_cursor?: string;
};

export type CursorPaged<T> = {
  data: T[];
  meta: CursorMeta;
};

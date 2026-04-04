type PaginationControlsProps = {
  limit: number;
  loading: boolean;
  hasMore: boolean;
  canGoBack: boolean;
  onPrev: () => void;
  onNext: () => void;
  onLimitChange: (limit: number) => void;
};

const LIMIT_OPTIONS = [25, 50, 100, 200];

export const PaginationControls = ({
  limit,
  loading,
  hasMore,
  canGoBack,
  onPrev,
  onNext,
  onLimitChange,
}: PaginationControlsProps) => (
  <div className="pagination-controls">
    <label>
      Rows
      <select
        className="search-input pagination-select"
        value={limit}
        onChange={(event) => onLimitChange(Number(event.target.value))}
        disabled={loading}
      >
        {LIMIT_OPTIONS.map((option) => (
          <option key={option} value={option}>
            {option}
          </option>
        ))}
      </select>
    </label>
    <div className="pagination-buttons">
      <button className="cta-button" onClick={onPrev} disabled={!canGoBack || loading}>
        Previous
      </button>
      <button className="cta-button" onClick={onNext} disabled={!hasMore || loading}>
        Next
      </button>
    </div>
  </div>
);

export function ScaleFilterControls({
  showCommonScales,
  showExoticScales,
  minCoveragePercent,
  onToggleCommon,
  onToggleExotic,
  onMinCoverageChange,
}: {
  showCommonScales: boolean;
  showExoticScales: boolean;
  minCoveragePercent: number;
  onToggleCommon: () => void;
  onToggleExotic: () => void;
  onMinCoverageChange: (percent: number) => void;
}) {
  return (
    <div className="row scale-filters" role="group" aria-label="Filter compatible scales">
      <button
        type="button"
        aria-pressed={showCommonScales}
        className={showCommonScales ? 'filter-toggle active' : 'filter-toggle'}
        onClick={onToggleCommon}
      >
        Common Scales
      </button>
      <button
        type="button"
        aria-pressed={showExoticScales}
        className={showExoticScales ? 'filter-toggle active' : 'filter-toggle'}
        onClick={onToggleExotic}
      >
        Exotic Scales
      </button>
      <label className="coverage-slider">
        Min coverage: {minCoveragePercent}%
        <input
          type="range"
          min={0}
          max={100}
          step={5}
          value={minCoveragePercent}
          onChange={(e) => onMinCoverageChange(Number(e.target.value))}
        />
      </label>
    </div>
  );
}

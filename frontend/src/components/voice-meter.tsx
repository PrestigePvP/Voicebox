const BAR_WEIGHTS = [0.3, 0.5, 0.7, 1.0, 0.7, 0.5, 0.3];

const VoiceMeter = ({ level }: { level: number }) => (
  <div className="flex items-center gap-[3px] h-8">
    {BAR_WEIGHTS.map((w, i) => (
      <div
        key={i}
        className="w-1 rounded-full bg-red-400 transition-[height] duration-75"
        style={{ height: `${Math.max(4, level * w * 32)}px` }}
      />
    ))}
  </div>
);

export default VoiceMeter;

interface OriginTagProps {
  origin: string;
}

export function OriginTag({ origin }: OriginTagProps) {
  return (
    <span className="rounded-tag bg-[rgba(8,9,11,0.72)] px-[7px] py-[3px] font-semibold text-[10.5px] text-white backdrop-blur-[4px]">
      {origin}
    </span>
  );
}

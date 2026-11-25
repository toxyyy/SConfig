export type Setting = { label: string; value: string; group?: string };
export function Overview({ values = [] }: { values?: Setting[] }) {
  const list = (items: Setting[]) => (
    <dl className="overview">
      {items.map((v) => (
        <div key={v.label}>
          <dt>{v.label}</dt>
          <dd>{v.value}</dd>
        </div>
      ))}
    </dl>
  );
  const general = values.filter((v) => !v.group || v.group === "general"),
    ads = values.filter((v) => v.group === "ads"),
    graphics = values.filter((v) => v.group === "graphics");
  return (
    <div className="settings-overview">
      {list(general)}
      {ads.length > 0 && (
        <details>
          <summary>Advanced ADS sensitivity</summary>
          {list(ads)}
        </details>
      )}
      {graphics.length > 0 && (
        <details>
          <summary>Graphics settings</summary>
          {list(graphics)}
        </details>
      )}
    </div>
  );
}

/** The Strata mark: the S of the Strata name (white; `size`: its height). */
export function Logo({ size = 20 }: { size?: number; mono?: boolean }) {
  return (
    <svg className="logo" viewBox="72 -1510 1208 1530" width={(size * 1208) / 1530} height={size} style={{ color: '#fff', flex: 'none' }} aria-hidden="true">
      <path fill="currentColor" transform="scale(1 -1)" d="M691 -20C1060 -20 1280 149 1280 432C1280 670 1113 814 825 877L696 907C563 937 476 981 476 1073C476 1156 549 1217 690 1217C832 1217 911 1154 921 1046H1260C1256 1322 1036 1510 691 1510C353 1510 111 1325 111 1049C111 825 267 696 524 637L682 600C845 563 923 518 923 433C923 338 833 273 687 273C526 273 419 347 411 492H72C78 148 318 -20 691 -20Z" />
    </svg>
  );
}

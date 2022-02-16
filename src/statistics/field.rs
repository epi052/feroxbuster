/// Enum representing fields whose updates need to be performed in batches instead of one at
/// a time
#[derive(Debug, Copy, Clone)]
pub enum StatField {
    /// Due to the necessary order of events, the number of requests expected to be sent isn't
    /// known until after `statistics::initialize` is called. This command allows for updating
    /// the `expected_per_scan` field after initialization
    ExpectedPerScan,

    /// Translates to `total_scans`
    TotalScans,

    /// Translates to `links_extracted`
    LinksExtracted,

    /// Translates to `extensions_collected`
    ExtensionsCollected,

    /// Translates to `total_expected`
    TotalExpected,

    /// Translates to `wildcards_filtered`
    WildcardsFiltered,

    /// Translates to `responses_filtered`
    ResponsesFiltered,

    /// Translates to `resources_discovered`
    ResourcesDiscovered,

    /// Translates to `initial_targets`
    InitialTargets,

    /// Translates to `directory_scan_times`; assumes a single append to the vector
    DirScanTimes,
}

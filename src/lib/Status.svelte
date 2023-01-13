<script lang="ts">
    import {listen} from "@tauri-apps/api/event"
    import {onDestroy} from "svelte"
    import LinearProgress from "@smui/linear-progress"

    let statusStr = 'Enter an AOF modpack location to begin downloading...'
    let error = false
    let downloading = false
    let progress = 0
    let extraction_total = 0
    let extraction_cur = 0
    let files_total = 0
    let files_cur = 0

    // Overall download lifecycle events
    listen('download_start', _ => {
        progress = 0
        downloading = true
        error = false
        statusStr = 'Download starting...';
    }).then(onDestroy)
    listen('download_error', event => {
        progress = 0
        downloading = false
        error = true
        statusStr = 'Download error: ' + event.payload
    }).then(onDestroy)
    listen('download_complete', _ => {
        error = false
        statusStr = 'Download complete.'
        downloading = false
        progress = 0
    })

    // Extraction-specific events
    listen('extraction_total', event => {
        extraction_total = event.payload as number
        statusStr = 'Starting extraction...'
    }).then(onDestroy)
    listen('extraction_cur', event => {
        extraction_cur = event.payload as number
        progress = extraction_cur / extraction_total * 0.25
        statusStr = `Extracting: ${extraction_cur} / ${extraction_total}`
    }).then(onDestroy)
    listen('extraction_complete', _ => {
        extraction_cur = extraction_total
        progress = 0.25
        statusStr = 'Extraction complete.'
    }).then(onDestroy)

    // Downloading-specific events
    listen('files_list', event => {
        files_total = (event.payload as string[]).length
        statusStr = 'Starting downloads...'
        console.log(statusStr)
    }).then(onDestroy)
    listen('file_complete', event => {
        files_cur = event.payload as number
        progress = files_cur / files_total * 0.75 + 0.25
        statusStr = `Downloaded ${files_cur} / ${files_total}`
        console.log(statusStr)
    })
</script>

<div class="flex flex-col gap-5 items-start">
    {#if error}
        <div class="text-red-600">
            {statusStr}
        </div>
    {:else}
        <div>
            {statusStr}
        </div>
    {/if}
    <LinearProgress {progress} closed="{!downloading}"/>
</div>

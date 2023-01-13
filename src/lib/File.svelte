<script lang="ts">
    import LinearProgress from '@smui/linear-progress'
    import {listen} from '@tauri-apps/api/event'
    import {onDestroy} from 'svelte'
    import {invoke} from "@tauri-apps/api/tauri";

    interface FileStatus {
        url: string
        msg: FileMsg | null
        curBytes: number
        totalBytes: number
        complete: boolean
    }

    interface FileMsg {
        msg: string
        error: boolean
    }

    export let url: string

    let statusStr = 'Not started.'
    let bytesDownloaded = 0
    let totalFileSize = 0
    let progress = -1
    let bytesStr = ''
    let complete = false

    invoke('get_status', {url}).then((status: FileStatus) => {
        if (status.url === url) {
            complete = status.complete
            progress = status.complete ? 1 : (status.totalBytes == 0 ? -1 : status.curBytes / status.totalBytes)
            statusStr = status.msg?.msg || statusStr
            bytesDownloaded = status.curBytes
            totalFileSize = status.totalBytes
        }
    });

    $: invoke('human_bytes', {bytes: bytesDownloaded}).then(bytesDownloadedStr => {
        invoke('human_bytes', {bytes: totalFileSize}).then(totalFileSizeStr => {
            bytesStr = `${bytesDownloadedStr} / ${totalFileSizeStr}`
        })
    })

    listen('file_status', event => {
        let status = event.payload as FileStatus
        if (status.url === url) {
            complete = status.complete
            progress = status.complete ? 1 : (status.totalBytes == 0 ? -1 : status.curBytes / status.totalBytes)
            statusStr = status.msg?.msg || statusStr
            bytesDownloaded = status.curBytes
            totalFileSize = status.totalBytes
        }
    }).then(onDestroy)
</script>

<div class="mx-2 my-1 flex flex-col gap-2 items-stretch border rounded p-2 border-gray-400"
     class:bg-gray-600={complete}>
    <div class="flex flex-row gap-5 items-center">
        <span class="flex-grow">{url}</span>
        <span>{statusStr}</span>
        <span>{bytesStr}</span>
    </div>
    <LinearProgress progress="{progress}" indeterminate="{progress < 0}"/>
</div>

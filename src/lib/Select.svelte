<script lang="ts">
    import {onDestroy} from "svelte"
    import {invoke} from "@tauri-apps/api/tauri"
    import {listen} from "@tauri-apps/api/event"
    import Textfield from '@smui/textfield'
    import Button from '@smui/button'

    let value = ''
    let valid = false
    let downloading = false

    $: invoke('path_is_valid', {value}).then((res: boolean) => valid = res)

    listen('download_start', _ => downloading = true).then(onDestroy)
    listen('download_error', _ => downloading = false).then(onDestroy)
    listen('download_complete', _ => downloading = false).then(onDestroy)

    async function select() {
        await invoke('modpack_select', {value})
    }
</script>

<div>
    <div class="flex flex-row items-center gap-5">
        <Textfield class="flex-grow" bind:value label="Modpack Path"></Textfield>
        <Button on:click={select} disabled="{!valid || downloading}">Begin</Button>
    </div>
</div>

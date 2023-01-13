<script lang="ts">
    import {listen} from "@tauri-apps/api/event";
    import File from "$lib/File.svelte";
    import {onDestroy} from "svelte";
    import VirtualList from '@sveltejs/svelte-virtual-list'

    let files = []

    listen('files_list', event => {
        files = event.payload as string[]
    }).then(onDestroy)
</script>

<div class="flex-grow border rounded border-gray-500 flex flex-col items-stretch overflow-y-scroll">
    <VirtualList items="{files}" let:item>
        <File url="{item}"/>
    </VirtualList>
</div>

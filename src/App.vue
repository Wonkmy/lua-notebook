<script setup lang="ts">
import { ref } from 'vue'
import MonacoEditor from 'monaco-editor-vue3'
import { invoke } from '@tauri-apps/api/core'
const currentKernel =
  ref("builtin")

async function switchKernel(
  e: Event
) {

  const value =
    (e.target as HTMLSelectElement)
      .value

  currentKernel.value =
    value

  await invoke(
    'switch_kernel',
    {
      kernelType: value
    }
  )
  await refreshGlobals()
}

const globals = ref<[string, string][]>([])
async function stopLua() {

  await invoke('restart_lua')
}
async function resetLua() {

  await invoke('reset_lua')

  globals.value = []

  for (const block of blocks.value) {

    block.output = ''
  }
}

async function refreshGlobals() {

  globals.value =
    await invoke<[string, string][]>(
      'get_globals'
    )
}

type CodeBlock = {
  id: number
  code: string
  output: string
}

const blocks = ref<CodeBlock[]>([
  {
    id: Date.now(),
    code: `a = 5\nprint(a)`,
    output: ''
  }
])

function addBlock() {
  blocks.value.push({
    id: Date.now(),
    code: `print("hello lua")`,
    output: ''
  })
}

function removeBlock(id: number) {
  // 至少保留一个代码块，避免页面空掉
  if (blocks.value.length <= 1) return

  blocks.value = blocks.value.filter(block => block.id !== id)
}

// function runBlock(block: CodeBlock) {
//   // 这里先做假输出，下一步再接 Rust + lua.exe
//   block.output = `准备执行：\n${block.code}`
// }
async function runBlock(block: CodeBlock) {

  try {

    const result = await invoke<string>('run_lua', {
      code: block.code
    })

    block.output = result
    await refreshGlobals()
  }
  catch (e) {

    block.output = String(e)
  }
}
</script>

<template>
  <div class="app">

    <div class="topbar">
      <button @click="addBlock">Add Block</button>
      <button @click="stopLua">Stop</button>
      <button @click="resetLua">Reset</button>
    </div>

    <div class="layout">

      <div class="main">

        <div class="notebook">
          <div
            v-for="block in blocks"
            :key="block.id"
            class="block"
          >
            <div class="block-header">
              <button @click="runBlock(block)">Run</button>
              <button @click="removeBlock(block.id)">Delete</button>
            </div>

            <MonacoEditor
              v-model:value="block.code"
              language="lua"
              theme="vs-dark"
              height="180px"
              :options="{
                fontSize: 16,
                minimap: { enabled: false },
                scrollBeyondLastLine: false
              }"
            />

            <pre class="output">{{ block.output }}</pre>
          </div>
        </div>

      </div>

      <div class="globals">

        <div class="globals-title">
          Globals
        </div>

        <div
          v-for="item in globals"
          :key="item[0]"
          class="global-item"
        >
          {{ item[0] }} : {{ item[1] }}
        </div>

      </div>

    </div>

  </div>
</template>

<style>
html, body, #app, .app {
  margin: 0;
  width: 100%;
  height: 100%;
  overflow: hidden;
}

.app {
  background: #1e1e1e;
  color: #ddd;
}

.topbar {
  height: 50px;
  background: #222;
  display: flex;
  align-items: center;
  padding: 0 10px;
  border-bottom: 1px solid #333;
}

button {
  height: 32px;
  margin-right: 8px;
  cursor: pointer;
}

.notebook {
  height: calc(100vh - 50px);
  overflow-y: auto;
  padding: 16px;
  box-sizing: border-box;
}

.block {
  margin-bottom: 18px;
  border: 1px solid #333;
  background: #252526;
}

.block-header {
  height: 42px;
  display: flex;
  align-items: center;
  padding: 0 10px;
  background: #2d2d2d;
  border-bottom: 1px solid #333;
}

.output {
  min-height: 40px;
  margin: 0;
  padding: 10px;
  background: #111;
  color: #9cdcfe;
  white-space: pre-wrap;
  font-size: 14px;
}
.layout {
  display: flex;
  height: 100%;
}

.main {
  flex: 1;
  overflow: hidden;
}

.globals {
  width: 240px;
  background: #181818;
  border-left: 1px solid #333;
  overflow-y: auto;
}

.globals-title {
  padding: 12px;
  font-weight: bold;
  border-bottom: 1px solid #333;
}

.global-item {
  padding: 8px 12px;
  border-bottom: 1px solid #222;
  font-size: 13px;
}
</style>

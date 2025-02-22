<template>
  <div class="outer-box">
    <div class="options-column">
      <img
          class="mb-3 about-logo-redirect"
          :src="HorizontalLogo"
          width="200"
          @click="openAboutPage"
        />
      <v-btn
        class="mt-6"
        size="large"
        rounded="lg"
        :prepend-icon="mdiVideo"
        :disabled="isProcessing"
        elevation="0"
        @click="openVideo"
      >
        Select Videos
      </v-btn>
      <UpscaleTypeOption
        :disabled="isProcessing"
        class="mt-2"
        @upscale-type-changed="setUpscaleType"
      />
      <UpscaleFactorOptions
        :disabled="isProcessing"
        class="mt-2"
        @upscale-factor-changed="updateUpscaleFactor" 
      />
      <UpscaleCodecOptions
        :disabled="isProcessing"
        class="mt-2"
        @upscale-codec-changed="updateUpscaleCodec"
      />
      <v-btn
        size="large"
        rounded="lg"
        class="mt-2"
        :disabled="isReadyToUpscale"
        elevation="0"
        width="310"
        @click="startProcessing"
      >
        {{
          isMultipleFiles ? "Upscale Selected Videos" : "Upscale Selected Video"
        }}
      </v-btn>
      <v-btn
        size="large"
        rounded="lg"
        class="mt-2"
        :disabled="isProcessing"
        elevation="0"
        @click="clearSelectedImage"
      >
        Clear
      </v-btn>
      <v-btn
        class="mt-2 cancel-button"
        size="large"
        rounded="lg"
        elevation="0"
        :disabled="!isProcessing"
        @click="cancelProcessing"
      >
        Cancel
      </v-btn>
      <div class="d-flex">
        <v-btn
          elevation="0"
          class="config-button"
          size="32"
          :icon="mdiMenu"
          @click="openConfig"
        ></v-btn>
      </div>
    </div>
    <div class="image-area mt-5" :class="{ 'text-center': !isMultipleFiles }">
      <h5 class="mb-2 path-text" v-if="imagePath">{{ imagePath }}</h5>
      <h5
        class="mb-2 path-text"
        :key="imagePath.path"
        v-for="imagePath in imagePaths"
      >
        <v-progress-circular
          v-if="!imagePath.isReady"
          v-show="showMultipleFilesProcessingIcon"
          indeterminate
          color="primary"
          size="16"
        />
        <v-icon
          v-else
          size="16"
          :icon="mdiVideoCheck"
          v-show="showMultipleFilesProcessingIcon"
        />
        <span class="ml-2">{{ imagePath.path }}</span>
        <v-divider />
      </h5>
      <v-progress-circular
        class="loading-gif"
        color="primary"
        indeterminate
        :size="128"
        :width="12"
        v-if="isProcessing && !isMultipleFiles"
      />
      <div
        class="file-drop-area mt-8"
        v-if="!imageBlob && !imagePaths.length"
        @click="openVideo"
      >
        Click to select videos or drop them here
      </div>
      <v-img
        class="image-src"
        :src="imageBlob"
        width="500"
        height="500"
        aspect-ratio="1"
        cover
        v-if="!!imageBlob"
      />
    </div>
  </div>
  <!-- status bar with a progress bar and a cancel button -->
  <div class="status-bar">
    <v-progress-linear
      :value="getProgress()"
      height="10"
      color="primary"
      class="progress-bar"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, Ref, computed } from "vue";
import HorizontalLogo from '../assets/reve-gui-horizontal.png';
import UpscaleTypeOption from "../components/UpscaleTypeOption.vue";
import UpscaleFactorOptions from "../components/UpscaleFactorOption.vue";
import UpscaleCodecOptions from "../components/UpscaleCodecOption.vue";
import { mdiVideo, mdiVideoCheck, mdiMenu } from "@mdi/js";
import { invoke } from "@tauri-apps/api/tauri";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/api/dialog";
import { WebviewWindow } from "@tauri-apps/api/window";
import { type } from "os";

interface ImagePathsDisplay {
  path: string;
  isReady: boolean;
}

type UpscaleType = "realesr-animevideov3";
type UpscaleFactor = 2 | 3 | 4;
type SegmentSize = 500 | 1000 | 2000;
type UpscaleCodec = "av1" | "x265";

const isProcessing = ref(false);
const imagePath = ref("");
const imagePaths: Ref<ImagePathsDisplay[]> = ref([]);
const imageBlob = ref("");
const upscaleFactor: Ref<UpscaleFactor> = ref(2);
const upscaleType: Ref<UpscaleType> = ref("realesr-animevideov3");
const upscaleCodec: Ref<UpscaleCodec> = ref("x265");
const segmentSize: Ref<SegmentSize> = ref(1000);
const isMultipleFiles = ref(false);
const showMultipleFilesProcessingIcon = ref(false);

// Computes if the user is ready to upscale the image. Used the simplify the DOM code.
const isReadyToUpscale = computed(() => {
  return !(
    (imagePath.value || imagePaths.value.length > 0) &&
    !isProcessing.value
  );
});

/**
 * Listens for file drops on the window and decides if is a single or multiple file upload.
 */
listen("tauri://file-drop", async (event) => {
  const files = event.payload as string[];
  if (!files.length || isProcessing.value) {
    return;
  }
  clearSelectedImage();
  if (files.length > 1) {
    showMultipleFilesProcessingIcon.value = false;
    isMultipleFiles.value = true;
    imagePaths.value = files
      .map((file) => {
        return {
          path: file,
          isReady: false,
        };
      })
      .filter((file) => {
        return (
          file.path.endsWith(".mkv") ||
          file.path.endsWith(".avi") ||
          file.path.endsWith(".mp4")
        );
      });
  } else {
    isMultipleFiles.value = false;
    if (
      !(
        files[0].endsWith(".mkv") ||
        files[0].endsWith(".avi") ||
        files[0].endsWith(".mp4")
      )
    ) {
      alert("Please select a valid video file.");
      return;
    }
  }
});

function openAboutPage() {
  // https://tauri.app/v1/guides/features/multiwindow#create-a-window-in-javascript
  const webview = new WebviewWindow("about-page", {
    height: 400,
    width: 500,
    title: "About",
    url: "/about",
  });
  // since the webview window is created asynchronously,
  // Tauri emits the `tauri://created` and `tauri://error` to notify you of the creation response
  webview.once("tauri://created", function () {
    // webview window successfully created
  });
  webview.once("tauri://error", function (err) {
    alert(err);
    // an error happened creating the webview window
  });
}

/**
 * Sets the upscale type.
 *
 * @param value - The upscale type. Available values are `general` and `digital`.
 */
function setUpscaleType(value: UpscaleType) {
  upscaleType.value = value;
}

/**
 * Sets the upscale factor.
 */
 function updateUpscaleFactor(value: UpscaleFactor) {
  upscaleFactor.value = value;
}

/**
 * Sets the upscale codec.
 */
function updateUpscaleCodec(value: UpscaleCodec) {
  upscaleCodec.value = value;
}

function openConfig() {
  // https://tauri.app/v1/guides/features/multiwindow#create-a-window-in-javascript
  const webview = new WebviewWindow("config-page", {
    height: 400,
    width: 500,
    title: "Config",
    url: "/config",
  });
  // since the webview window is created asynchronously,
  // Tauri emits the `tauri://created` and `tauri://error` to notify you of the creation response
  webview.once("tauri://created", function () {
    // webview window successfully created
  });
  webview.once("tauri://error", function (err) {
    alert(err);
    // an error happened creating the webview window
  });
}

/**
 * Clears the selected image and some other variables.
 */
function clearSelectedImage() {
  imagePath.value = "";
  imagePaths.value = [];
  imageBlob.value = "";
  showMultipleFilesProcessingIcon.value = false;
  isMultipleFiles.value = false;
}

/**
 * Opens the image file dialog from Tauri.
 *
 * It is used in the single and multiple file selector.
 */
async function openVideo() {
  // Open a selection dialog for image files
  const selected = await open({
    multiple: true,
    filters: [
      {
        name: "",
        extensions: ["avi", "mkv", "mp4"],
      },
    ],
  });
  if (Array.isArray(selected) && selected.length > 1) {
    clearSelectedImage();
    isMultipleFiles.value = true;
    showMultipleFilesProcessingIcon.value = false;
    imagePaths.value = selected.map((path) => {
      return {
        path,
        isReady: false,
      };
    });
  } else if (selected === null) {
    // user cancelled the selection
  } else {
    clearSelectedImage();
    isMultipleFiles.value = false;
    imagePath.value = selected[0];
  }
}

/**
 * Runs the correct single or multiple file processing function.
 *
 * It is used to control the code flow.
 */
function startProcessing() {
  if (isMultipleFiles.value) {
    upscaleMultipleImages();
  } else {
    upscaleSingleImage();
  }
}

/**
 * Upscales multiple images function.
 *
 * It will ask the user to select a folder to save the upscaled images.
 *
 * It will update the `isReady` property of the `imagePaths` array to true when the image is ready.
 */
function upscaleMultipleImages() {
  const outputFolder = open({
    directory: true,
  });
  if (outputFolder === null) {
    return;
  }
  isProcessing.value = true;
  showMultipleFilesProcessingIcon.value = true;
  try {
    for (let i = 0; i < imagePaths.value.length; i++) {
      let outputFile: string = imagePaths.value[i].path;
    // Replaces the filename of file in the given path with '<path><filename>-<upscale_factor>x.<codec>.<extension>'
      outputFile = outputFile.replace(
        /(.*)[\/\\]([^\/\\]+)\.([^\/\\]+)$/,
        `$1/$2-${upscaleFactor.value}x.${upscaleCodec.value}.$3`
      );      
      invoke("upscale_video", {
        path: imagePaths.value[i].path,
        savePath: outputFile,
        upscaleFactor: upscaleFactor.value,
        upscaleType: upscaleType.value,
        upscaleCodec: upscaleCodec.value,
        segmentSize: segmentSize.value,
      });
      imagePaths.value[i].isReady = true;
    }
  } catch (err: any) {
    showMultipleFilesProcessingIcon.value = false;
    invoke("write_log", { message: err.toString() });
    alert(err);
  } finally {
    isProcessing.value = false;
  }
}

/**
 * Upscales a single image.
 *
 * It will ask the user to select a file name and location to save the upscaled image.
 *
 * After the image is upscaled, it will send a `alert` to the user.
 */
function upscaleSingleImage() {
  if (imagePath.value === "") {
    alert("No video selected");
    return;
  }
  const imageSavePath = imagePath.value.replace(
      /(.*)[\/\\]([^\/\\]+)\.([^\/\\]+)$/,
      `$1/$2-${upscaleFactor.value}x.${upscaleCodec.value}.$3`
    );
  if (imageSavePath === null) {
    // user cancelled the selection
    return;
  }
  isProcessing.value = true;
  try {
    invoke("upscale_video", {
      path: imagePath.value,
      savePath: imageSavePath,
      upscaleFactor: upscaleFactor.value,
      upscaleType: upscaleType.value,
      upscaleCodec: upscaleCodec.value,
      segmentSize: segmentSize.value,
    });
  } catch (err: any) {
    invoke("write_log", { message: err.toString() });
    alert(err);
  } finally {
    isProcessing.value = false;
  }
}

function cancelProcessing() {
  isProcessing.value = false;
  showMultipleFilesProcessingIcon.value = false;
}

/** function to get the progress of the current processing and update the progress bar */
async function getProgress() {
  const progress = await invoke("get_progress");
  if (progress === null) {
    return;
  }
  if (progress === 100) {
    isProcessing.value = false;
    showMultipleFilesProcessingIcon.value = false;
  }
  progress;
}

</script>

<style scoped lang="scss">
.loading-gif {
  z-index: 1;
  margin-left: -70px;
  margin-top: 190px;
  position: fixed;
}
.image-src {
  border-radius: 24px;
  border: 2px solid rgba($color: #969696, $alpha: 0.4);
}
.image-area {
  min-width: 500px;
  min-height: 500px;
}

.path-text {
  font-size: 14px;
  font-weight: normal;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.file-drop-area {
  min-width: 500px;
  min-height: 500px;
  border: 2px dashed rgba($color: #969696, $alpha: 0.4);
  border-radius: 24px;
  display: flex;
  justify-content: center;
  align-items: center;
  flex-direction: column;
  cursor: pointer;
}

.outer-box {
  display: flex;
  flex-direction: row;
  justify-content: space-between;
  width: 800px;
  height: 100%;
}

.about-logo-redirect {
  margin-left: 2px;
  margin-bottom: 0px !important;
  height: 30px;
  cursor: pointer;
}

.config-button {
  margin-top: 20px;
}
.options-column {
  display: flex;
  flex-direction: column;
  align-items: stretch;
  justify-content: center;
  width: 100%;
  height: 100%;
  padding: 20px;
  box-sizing: border-box;
}

.progress-bar {
  width: 100%;
  height: 20px;
  border-radius: 10px;
  overflow: hidden;
  margin-top: 10px;
}
</style>

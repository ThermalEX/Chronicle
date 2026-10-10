import {expect,it} from "vitest";import {createSSRApp} from "vue";import {renderToString} from "@vue/server-renderer";
import WallpaperGallery from "./WallpaperGallery.vue";
it("offers a dashed add target with no old image buttons",async()=>{
  const html=await renderToString(createSSRApp(WallpaperGallery,{paths:[],selectedIndex:0}));
  expect(html).toContain('data-testid="add-wallpapers"');expect(html).not.toContain("选择图片</button>");expect(html).not.toContain("移除图片</button>");
});
it("renders individual named image controls and delete labels",async()=>{
  const html=await renderToString(createSSRApp(WallpaperGallery,{paths:["C:/a.png","C:/b.png"],selectedIndex:1}));
  expect(html).toContain("a.png");expect(html).toContain("b.png");expect(html).toContain('aria-label="移除图片 b.png"');expect(html).toContain('aria-pressed="true"');
});

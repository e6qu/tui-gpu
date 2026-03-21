## Project: Contour (Qt/OpenGL Terminal)

### Stack and architecture
- Qt Quick GUI shell handles windowing/IME; rendering explicitly forces OpenGL (`ContourGuiApp.cpp` sets `QSGRendererInterface::OpenGL`).
- `src/contour/display/OpenGLRenderer.cpp` encapsulates all GL state: VAOs/VBOs per primitive (rectangles, textured glyph quads) and shader creation via `ShaderConfig`.
- Font atlas + rasterization provided by `vtrasterizer`, feeding GPU uploads via Qt's `QOpenGLTexture`.

### How Contour interfaces with the GPU
- During initialization, `initializeRectRendering` / `initializeTextureRendering` allocate VAOs and stream buffers, enabling attributes for vertex positions, UVs, and colors.
- Rendering loop clears or loads the framebuffer, uploads dirty rectangles and glyph vertices (with `glBufferData(GL_STREAM_DRAW)`), and issues `glDrawArrays` calls.
- Glyph atlas updates triggered by scheduled executions; textures uploaded via `glTexSubImage2D` using Contour's `atlas::AtlasBackend`.
- Additional protocols (Sixel, Kitty graphics) convert pixel data into GPU textures, letting TUIs mix text and bitmap panels.

### Sample code (Contour rectangle draw)
```cpp
// src/contour/display/OpenGLRenderer.cpp:552-575
if (!_rectBuffer.empty())
{
    bound(*_rectShader, [&]() {
        _rectShader->setUniformValue(_rectProjectionLocation, mvp);
        _rectShader->setUniformValue(_rectTimeLocation, timeValue);
        glBindVertexArray(_rectVAO);
        glBindBuffer(GL_ARRAY_BUFFER, _rectVBO);
        glBufferData(GL_ARRAY_BUFFER,
                     static_cast<GLsizeiptr>(_rectBuffer.size() * sizeof(GLfloat)),
                     _rectBuffer.data(),
                     GL_STREAM_DRAW);
        glDrawArrays(GL_TRIANGLES, 0, static_cast<GLsizei>(_rectBuffer.size() / 7));
        glBindVertexArray(0);
    });
    _rectBuffer.clear();
}
```
This block streams all pending rectangles (cursor, selection, damage) into a single GL buffer and draws them as triangles using the currently bound shader.

### Notes for agents
- JSON-RPC + OSC interfaces can command Contour to spawn panes or inject input; overlays could be implemented by feeding additional geometry buffers into `_rectBuffer`.
- Because Qt supplies high-quality IME + clipboard integration, coding agents focusing on multilingual input can rely on Contour while customizing rendering through the OpenGL layer.
- Lack of non-OpenGL backends means advanced compute work must be emulated via multipass fragment shaders, but the renderer already exposes hooks for shader effects (e.g., outlines, blur).

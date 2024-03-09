//
//  VIewportMTKVIew.swift
//  Monocurl
//
//  Created by Manu Bhat on 10/30/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

import Foundation
import Metal
import MetalKit
import SwiftUI


extension MTLDevice {
    var supportedSampleCount: Int {
        
        var finalSampleCount = 1;
        for i in stride(from: 16, to: 0, by: -1) {
            if (self.supportsTextureSampleCount(i)){
                finalSampleCount = i;
                break;
            }
        }
        
        return finalSampleCount
    }
}

enum Shader: Hashable {
    case tri
    case lin
    case dot;
}

fileprivate let shadersList: [(shader: Shader, vertex: String, fragment: String)] = [
    (.tri, "tri_vert_shader", "tri_frag_shader"),
    (.lin, "lin_vert_shader", "lin_frag_shader"),
    (.dot, "dot_vert_shader", "dot_frag_shader"),
]

class MetalController: NSObject {
    static var shaders: [Shader: MTLRenderPipelineState] = [:]
    
    fileprivate var gpu: MTLDevice?
    fileprivate var commandQueue: MTLCommandQueue?
    fileprivate let semaphore = DispatchSemaphore(value: 1)
    
    fileprivate var drawSize: simd_float2 = .zero
    fileprivate var inletSize: simd_float2 = .zero
    
    fileprivate var camera = matrix_identity_float4x4
    fileprivate var fullProjection = matrix_identity_float4x4;
    fileprivate var standardProjection = matrix_identity_float4x4;
    fileprivate var viewportMap = matrix_identity_float4x4
    
    fileprivate var dotRenderer: DotRenderer!
    fileprivate var linRenderer: LinRenderer!
    fileprivate var triRenderer: TriRenderer!

    fileprivate(set) var bufferManager: BufferManager!
    fileprivate(set) var textureManager: TextureManager!
    
    fileprivate(set) var depthStencil: MTLDepthStencilState!
    
    var cache: ViewportCache!
    
    func initialize(gpu: MTLDevice, cache: ViewportCache) {
        self.cache = cache;

        commandQueue = gpu.makeCommandQueue();

        dotRenderer = DotRenderer(gpu: gpu, bufferManager: bufferManager)
        linRenderer = LinRenderer(gpu: gpu, bufferManager: bufferManager);
        triRenderer = TriRenderer(gpu: gpu, bufferManager: bufferManager, textureManager: textureManager);
        
        let stencil = MTLDepthStencilDescriptor()
        stencil.isDepthWriteEnabled = true;
        stencil.depthCompareFunction = .lessEqual
        self.depthStencil = gpu.makeDepthStencilState(descriptor: stencil);
        
        if (Self.shaders.isEmpty) {
            for shader in shadersList {
                guard let pipeline = self.buildRenderPipelineWith(gpu: gpu, vertexFunction: shader.vertex, fragmentFunction: shader.fragment) else {
                    continue
                }
                
                Self.shaders[shader.shader] = pipeline;
            }
        }
    }
    
    fileprivate func generateCommandEncoder(command: MTLCommandBuffer, for pass: MTLRenderPassDescriptor) -> MTLRenderCommandEncoder? {
        let pass = pass
        let bg = self.cache.background
        pass.colorAttachments[0].clearColor = MTLClearColor(red: Double(bg.x), green: Double(bg.y), blue: Double(bg.z), alpha: Double(bg.w))
        return command.makeRenderCommandEncoder(descriptor: pass);
    }
    
    fileprivate func encode(with encoder: MTLRenderCommandEncoder) {
        mc_rwlock_reader_lock(self.cache.lock);
       
        encoder.setFrontFacing(.counterClockwise)
        encoder.setCullMode(.back)
        encoder.setDepthClipMode(.clip)
        encoder.setDepthStencilState(self.depthStencil)
        
//        encoder.setTriangleFillMode(.lines)
        
        let mv = self.camera // * matrix_identity_float4x4
        let p = self.fullProjection;
        /* let mvp = p * mv; */
        let normal = simd_inverse(simd_transpose(mv))

        // we use the pointer to avoid memory leaks...
        var z_offset: Float = 0
        for i in 0 ..< self.cache.ref.pointee.mesh_count {
            z_offset = self.encode(mesh: self.cache.ref.pointee.meshes[i]!, with: encoder, mv: mv, p: p, normal: normal, z_offset: z_offset);
        }

        mc_rwlock_reader_unlock(self.cache.lock);
    }

    private func encode(mesh: UnsafeMutablePointer<tetramesh>, with encoder: MTLRenderCommandEncoder, mv: simd_float4x4, p: simd_float4x4, normal: simd_float4x4, z_offset: Float) -> Float {
        var z_offset = z_offset
        if self.triRenderer.encode(mesh: mesh, mv: mv, p: p, normal: normal, z_offset: z_offset, with: encoder) {
            z_offset += 1e-6
        }
        if self.linRenderer.encode(mesh: mesh, inletSize: self.inletSize, mv: mv, p: p, normal: normal, z_offset: z_offset, with: encoder) {
            z_offset += 1e-6
        }
        if self.dotRenderer.encode(mesh: mesh, viewportSize: self.drawSize, mv: mv, p: p, normal: normal, z_offset: z_offset, with: encoder) {
            z_offset += 1e-6
        }
        
        if (mesh.pointee.modded != 0) {
            mesh.pointee.modded = 0;
        }
        
        return z_offset
    }
    
    //this will be done via an update from c...
    func initializeProjections() {
        let z = simd_normalize(self.cache.forward)
        let x = simd_normalize(simd_cross(z, self.cache.up))
        let y = simd_cross(x, z)
        
        /* orthonormal so inverse is transpose */
        let inv_rotation = matrix_float4x4(simd_float4(x, 0), simd_float4(y, 0), -simd_float4(z, 0), simd_float4(0,0,0,1))
        let full_rotation = inv_rotation.transpose
        
        /* we're right handed but metal is left handed, so have to do some offsets */
        let origin = self.cache.origin
        self.camera = full_rotation *
                      matrix_float4x4(columns: (simd_float4(1,0,0,0),//x
                                                simd_float4(0,1,0,0),//y
                                                simd_float4(0,0,1,0),//z
                                                simd_float4(-origin.x,-origin.y, -origin.z,1)
                                               ) //w,
                                      )
        
        //z flattening is done by camera?
        let aspectRatio = Float(self.cache.aspectRatio);
        let f = self.cache.far;
        let n = self.cache.near;
        self.standardProjection = matrix_float4x4(columns: (
            simd_float4(1,0,0,0),//x
            simd_float4(0,aspectRatio,0,0),//y
            simd_float4(0,0,-f / (f - n), -1),//z
            simd_float4(0,0,-f * n / (f - n),0)) //w
        )
        
        let pointScale = Float(NSScreen.main?.backingScaleFactor ?? 1);
        let internalRect = ViewportFrame<EmptyView>.internal_rect(size: CGSize(width: Double(self.drawSize.x / pointScale), height: Double(self.drawSize.y / pointScale)), aspectRatio: self.cache.aspectRatio)
        
        self.inletSize = pointScale * simd_float2(Float(internalRect.width), Float(internalRect.height))
        //(1,-1) -> (internal rect coordinates)
        self.viewportMap = matrix_float4x4(columns: (
            simd_float4(self.inletSize.x / self.drawSize.x,0,0,0),//x
            simd_float4(0,self.inletSize.y / self.drawSize.y,0,0),//y
            simd_float4(0,0,1,0),//z
            simd_float4(0,0,0,1)) //w
        )
        
        self.fullProjection = self.viewportMap * self.standardProjection;
    }
    
    func buildRenderPipelineWith(gpu: MTLDevice, vertexFunction: String, fragmentFunction: String) -> MTLRenderPipelineState? {
        // Create a new pipeline descriptor
        let pipelineDescriptor = MTLRenderPipelineDescriptor()

        // Setup the shaders in the pipeline
        let library = gpu.makeDefaultLibrary()
        pipelineDescriptor.vertexFunction = library?.makeFunction(name: vertexFunction)
        pipelineDescriptor.fragmentFunction = library?.makeFunction(name: fragmentFunction)

        pipelineDescriptor.rasterSampleCount = gpu.supportedSampleCount;
        
        pipelineDescriptor.colorAttachments[0].pixelFormat = .bgra8Unorm;
        pipelineDescriptor.colorAttachments[0].isBlendingEnabled = true

        pipelineDescriptor.colorAttachments[0].rgbBlendOperation = .add
        pipelineDescriptor.colorAttachments[0].alphaBlendOperation = .add

        pipelineDescriptor.colorAttachments[0].sourceRGBBlendFactor = .sourceAlpha
        pipelineDescriptor.colorAttachments[0].sourceAlphaBlendFactor = .sourceAlpha

        pipelineDescriptor.colorAttachments[0].destinationRGBBlendFactor = .oneMinusSourceAlpha
        pipelineDescriptor.colorAttachments[0].destinationAlphaBlendFactor = .oneMinusSourceAlpha
        
        pipelineDescriptor.depthAttachmentPixelFormat = .depth32Float;
        
        pipelineDescriptor.isRasterizationEnabled = true;


        // Compile the configured pipeline descriptor to a pipeline state object
        return try? gpu.makeRenderPipelineState(descriptor: pipelineDescriptor)
    }
}

class ViewportMTKViewDelegate: MetalController, MTKViewDelegate {
    
    func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {
        self.drawSize = simd_float2(Float(size.width), Float(size.height))
        self.initializeProjections();
    }
   
    override func initialize(gpu: MTLDevice, cache: ViewportCache) {
        bufferManager = BufferManager(gpu: gpu)
        textureManager = TextureManager(gpu: gpu)
        Monocurl.bufferManager = bufferManager;
        Monocurl.textureManager = textureManager;
        
        super.initialize(gpu: gpu, cache: cache)
    }
    
    func draw(in view: MTKView) {
        //runtimeline and grab meshes
        semaphore.wait()
        
        guard let commandBuffer = commandQueue?.makeCommandBuffer() else {
            semaphore.signal()
            return
        }
        
        guard let renderPassDescriptor = view.currentRenderPassDescriptor else {
            semaphore.signal()
            return
        }
        
        guard let renderEncoder = generateCommandEncoder(command: commandBuffer, for: renderPassDescriptor) else {
            semaphore.signal()
            return
        }
                
        guard let drawable = view.currentDrawable else {
            semaphore.signal()
            return
        }
        
        self.encode(with: renderEncoder);
        
        renderEncoder.endEncoding();
        
        commandBuffer.addCompletedHandler { _ in
            self.semaphore.signal()
        }
        commandBuffer.present(drawable)
        commandBuffer.commit();
    }
}

//technically this is part of toolbar because of the view
//hierarchy
//might want to change in the future???
class ExportController: MetalController, ObservableObject {
    
    private var texture: MTLTexture!
    private var resolveTexture: MTLTexture!
    private var pixelBuffer: UnsafeMutableRawPointer!
    
    var successMessage: Binding<Bool>!
    var errorMessage: Binding<String?>!
    
    var renderPass: MTLRenderPassDescriptor!;
    
    func export(gpu: MTLDevice, cache: ViewportCache, width: Int, height: Int, fps: Int, upf: Int, save: URL) {
        self.cache = cache;
        self.gpu = gpu;
        
        //steal from viewport's managers
        bufferManager = Monocurl.bufferManager;
        textureManager = Monocurl.textureManager;
        Monocurl.exportManager = self;
        
        self.drawSize = simd_float2(Float(width), Float(height))
        self.inletSize = self.drawSize;
       
        (self.texture, self.resolveTexture) = textureManager.renderTexture(width: width, height: height);
        
        
        let renderPassDescriptor = MTLRenderPassDescriptor();
        renderPassDescriptor.colorAttachments[0].texture = texture;
        renderPassDescriptor.colorAttachments[0].resolveTexture = self.resolveTexture;
        renderPassDescriptor.colorAttachments[0].loadAction = .clear;
        renderPassDescriptor.colorAttachments[0].clearColor = .init(red: 0, green: 0, blue: 0, alpha: 1)
        renderPassDescriptor.colorAttachments[0].storeAction = .multisampleResolve
        
        renderPassDescriptor.depthAttachment.texture = textureManager.depthTexture(width: width, height: height)
        renderPassDescriptor.depthAttachment.loadAction = .clear
        renderPassDescriptor.depthAttachment.storeAction = .dontCare
        renderPassDescriptor.depthAttachment.clearDepth = 1
       
        self.renderPass = renderPassDescriptor;
        
        //pixel values no larger than 32 bpp
        self.pixelBuffer = .allocate(byteCount: width * height * MemoryLayout<Int32>.stride, alignment: MemoryLayout<Int32>.alignment)
        
        super.initialize(gpu: gpu, cache: cache);
        
        //start export process
        timeline_start_export(cache.ref.pointee.handle.pointee.timeline, path(for: save).utf8CStringPointer, UInt32(width), UInt32(height), UInt32(fps), UInt32(upf))
    }
    //so
    
    func draw() {
        semaphore.wait()
    
        guard let commandBuffer = commandQueue?.makeCommandBuffer() else {
            semaphore.signal()
            return
        }
        
        guard let renderEncoder = generateCommandEncoder(command: commandBuffer, for: self.renderPass) else {
            semaphore.signal()
            return
        }

        self.initializeProjections()
        self.encode(with: renderEncoder);
        
        renderEncoder.endEncoding();
        
        commandBuffer.addCompletedHandler { _ in
            self.resolveTexture.getBytes(self.pixelBuffer, bytesPerRow: self.texture.width * MemoryLayout<Int32>.stride, from: MTLRegionMake2D(0, 0, self.texture.width, self.texture.height), mipmapLevel: 0)
            
            timeline_write_frame(self.cache.ref.pointee.handle.pointee.timeline, self.pixelBuffer)
            self.semaphore.signal()
        }
        commandBuffer.commit();
    }
    
    func exportFinish(error: UnsafePointer<CChar>!) {
        //hide export panel
        if error == nil {
            successMessage.wrappedValue = true;
        }
        else {
            errorMessage.wrappedValue = String(cString: error);
        }
    }
    
    deinit {
        self.pixelBuffer?.deallocate()
    }
}

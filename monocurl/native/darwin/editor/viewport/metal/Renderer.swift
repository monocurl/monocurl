//
//  DotIndexBufferManager.swift
//  Monocurl
//
//  Created by Manu Bhat on 11/1/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

import Foundation
import Metal

fileprivate let referenceWidth = 1480;

//all differ ever so slightly
//so a protocol would make things weird
//(and it would have associated types... so not really many benefits)
public final class DotRenderer {
    private static let startCapacity: Int = 8;

    private let gpu: MTLDevice;
    private var indexBuffer: MTLBuffer
    private let bufferManager: BufferManager
    
    init?(gpu: MTLDevice, bufferManager: BufferManager) {
        self.gpu = gpu;
        
        let indices = Self.indices(forVertexCount: Self.startCapacity)
        guard let buffer = gpu.makeBuffer(bytes: indices, length: indices.count * MemoryLayout<UInt16>.stride) else {
            return nil
        }
        
        self.indexBuffer = buffer;
        self.bufferManager = bufferManager;
    }
    
    func encode(mesh pointer: UnsafeMutablePointer<tetramesh>, viewportSize: simd_float2, mv: matrix_float4x4, p: matrix_float4x4, normal: matrix_float4x4, z_offset: Float, with encoder: MTLRenderCommandEncoder) -> Bool {
        var mesh = pointer.pointee;

        guard mesh.dots != nil else {
            return false
        }
        
        if mesh.dot_handle == 0 {
            pointer.pointee.dot_handle = bufferManager.registerID()
            mesh = pointer.pointee
        }
        
        //write in data if needed
        if (mesh.modded != 0) {
            var count: Int = 0
            withUnsafeMutablePointer(to: &count) { countPtr in
                let buffer = dot_buffer_pointer_for(pointer, countPtr);
                self.bufferManager.write(bytes: buffer!, length: countPtr.pointee * MemoryLayout<dot_vert_in>.stride, into: mesh.dot_handle);
                buffer?.deallocate();
            }
        }
        
        guard let bufferTuple = self.bufferManager.buffer(for: mesh.dot_handle), let buffer = bufferTuple.buffer else {
            return false;
        }
        
        
        return self.encode(vertices: buffer, length: bufferTuple.length, vertexUniform: dot_vert_uniform(vertex_count: mesh.uniform.dot_vertex_count, viewport_size: viewportSize, radius: mesh.uniform.dot_radius, mv: mv, p: p, normal: normal, z_offset: z_offset), fragmentUniform: dot_frag_uniform(opacity: mesh.uniform.opacity, gloss: mesh.uniform.gloss), with: encoder)
    }
    
    func encode(vertices: MTLBuffer, length: Int, vertexUniform: dot_vert_uniform, fragmentUniform: dot_frag_uniform, with encoder: MTLRenderCommandEncoder) -> Bool {
        if (fragmentUniform.opacity < Float.ulpOfOne || length == 0) {
            return false
        }
                
        withUnsafePointer(to: fragmentUniform) { fragmentPointer in
            withUnsafePointer(to: vertexUniform) { vertexPointer in
                encoder.setRenderPipelineState(ViewportMTKViewDelegate.shaders[.dot]!)
                
                encoder.setVertexBuffer(vertices, offset: 0, index: 0)
                encoder.setVertexBytes(vertexPointer, length: MemoryLayout<dot_vert_uniform>.stride, index: 1);
                encoder.setFragmentBytes(fragmentPointer, length: MemoryLayout<dot_frag_uniform>.stride, index: 0);
                
                encoder.drawIndexedPrimitives(
                    type: .triangle,
                    indexCount: Self.indicesCount(forVertexCount: Int(vertexUniform.vertex_count)),
                    indexType: .uint16,
                    indexBuffer: self.sizedIndexBuffer(vertexCount: Int(vertexUniform.vertex_count)),
                    indexBufferOffset: 0,
                    instanceCount: length / MemoryLayout<dot_vert_in>.stride
                )
            }
        }
        
        return true
    }
    
    private func sizedIndexBuffer(vertexCount count: Int) -> MTLBuffer {
        if (Self.indicesCount(forVertexCount: count) > self.indexBuffer.length / MemoryLayout<UInt16>.stride) {
            let indices = Self.indices(forVertexCount: count);
            self.indexBuffer = gpu.makeBuffer(bytes: indices, length: indices.count * MemoryLayout<UInt16>.stride)!
        }
        
        return self.indexBuffer;
    }
    
    private static func indices(forVertexCount vertexCount: Int) -> [UInt16] {
        var ret: [UInt16] = [];
        for i in 1 ..< vertexCount - 1 {
            ret.append(0)
            ret.append(UInt16(i));
            ret.append(UInt16(i + 1))
        }
        
        return ret;
    }
    
    private static func indicesCount(forVertexCount vertexCount: Int) -> Int {
        return 3 * (vertexCount - 2);
    }
}

fileprivate let verticesPerLine = 6;
fileprivate let lineIndices: [UInt16]  = [
    0, 2, 1,
    1, 2, 4,
    1, 4, 3,
    3, 4, 5,
]

public final class LinRenderer {
    private let gpu: MTLDevice;
    private let indexBuffer: MTLBuffer
    private let bufferManager: BufferManager
    
    init?(gpu: MTLDevice, bufferManager: BufferManager) {
        self.gpu = gpu;
        
        guard let buffer = gpu.makeBuffer(bytes: lineIndices, length: lineIndices.count * MemoryLayout<UInt16>.stride) else {
            return nil
        }
        
        self.indexBuffer = buffer;
        self.bufferManager = bufferManager;
    }
    
    func encode(mesh pointer: UnsafeMutablePointer<tetramesh>, inletSize: simd_float2, mv: matrix_float4x4, p: matrix_float4x4, normal: matrix_float4x4, z_offset: Float, with encoder: MTLRenderCommandEncoder) -> Bool {
        var mesh = pointer.pointee;

        guard mesh.lins != nil else {
            return false
        }
        
        if mesh.lin_handle == 0 {
            pointer.pointee.lin_handle = bufferManager.registerID()
            mesh = pointer.pointee
        }
        
        //write in data if needed
        if (mesh.modded != 0) {
            var count: Int = 0
            withUnsafeMutablePointer(to: &count) { countPtr in
                let buffer = lin_buffer_pointer_for(pointer, countPtr);
                self.bufferManager.write(bytes: buffer!, length: 6 * countPtr.pointee * MemoryLayout<lin_vert_in>.stride, into: mesh.lin_handle);
                buffer?.deallocate();
            }
        }
        
        guard let bufferTuple = self.bufferManager.buffer(for: mesh.lin_handle), let buffer = bufferTuple.buffer else {
            return false;
        }
        
        return self.encode(vertices: buffer, length: bufferTuple.length, vertexUniform: lin_vert_uniform(inlet_size: inletSize, radius: mesh.uniform.stroke_radius * inletSize.x / Float(referenceWidth), max_miter_scale: mesh.uniform.stroke_miter_radius_scale, mv: mv, p: p, normal: normal, z_offset: z_offset), fragmentUniform: lin_frag_uniform(opacity: mesh.uniform.opacity, gloss: mesh.uniform.gloss), with: encoder)
    }
    
    func encode(vertices: MTLBuffer, length: Int, vertexUniform: lin_vert_uniform, fragmentUniform: lin_frag_uniform, with encoder: MTLRenderCommandEncoder) -> Bool {
        if (fragmentUniform.opacity < Float.ulpOfOne || length == 0) {
            return false
        }
        
        withUnsafePointer(to: fragmentUniform) { fragmentPointer in
            withUnsafePointer(to: vertexUniform) { vertexPointer in
                encoder.setRenderPipelineState(ViewportMTKViewDelegate.shaders[.lin]!)
                
                encoder.setVertexBuffer(vertices, offset: 0, index: 0)
                encoder.setVertexBytes(vertexPointer, length: MemoryLayout<lin_vert_uniform>.stride, index: 1);
                encoder.setFragmentBytes(fragmentPointer, length: MemoryLayout<lin_frag_uniform>.stride, index: 0);
               
                encoder.drawIndexedPrimitives(
                    type: .triangle,
                    indexCount: lineIndices.count,
                    indexType: .uint16,
                    indexBuffer: self.indexBuffer,
                    indexBufferOffset: 0,
                    instanceCount: length / (MemoryLayout<lin_vert_in>.stride * verticesPerLine)
                )
            }
        }
        
        return true
    }
}

public final class TriRenderer {

    private let gpu: MTLDevice;
    private let bufferManager: BufferManager;
    private let textureManager: TextureManager
    
    init(gpu: MTLDevice, bufferManager: BufferManager, textureManager: TextureManager) {
        self.gpu = gpu
        self.bufferManager = bufferManager
        self.textureManager = textureManager
    }

    func encode(mesh pointer: UnsafeMutablePointer<tetramesh>, mv: matrix_float4x4, p: matrix_float4x4, normal: matrix_float4x4, z_offset: Float, with encoder: MTLRenderCommandEncoder) -> Bool {
        var mesh = pointer.pointee;

        guard mesh.tris != nil else {
            return false
        }
        
        if mesh.tri_handle == 0 {
            pointer.pointee.tri_handle = bufferManager.registerID()
            mesh = pointer.pointee
        }
        
        //write in data if needed
        if (mesh.modded != 0) {
            var count: Int = 0
            withUnsafeMutablePointer(to: &count) { countPtr in
                let buffer = tri_buffer_pointer_for(pointer, countPtr);
                self.bufferManager.write(bytes: buffer!, length: 3 * countPtr.pointee * MemoryLayout<tri_vert_in>.stride, into: mesh.tri_handle);
                buffer?.deallocate();
            }
        }
        
        guard let bufferTuple = self.bufferManager.buffer(for: mesh.tri_handle),
              let buffer = bufferTuple.buffer,
              let texture = self.textureManager.texture(for: mesh.texture_handle) else {
            return false;
        }
        
        return self.encode(vertices: buffer, length: bufferTuple.length, vertexUniform: tri_vert_uniform(mv: mv, p: p, normal: normal, z_offset: z_offset), fragmentUniform: tri_frag_uniform(opacity: mesh.uniform.opacity, gloss: mesh.uniform.gloss), texture: texture, with: encoder)
    }

    func encode(vertices: MTLBuffer, length: Int, vertexUniform: tri_vert_uniform, fragmentUniform: tri_frag_uniform, texture: MTLTexture, with encoder: MTLRenderCommandEncoder) -> Bool {
        if (fragmentUniform.opacity < Float.ulpOfOne || length == 0) {
            return false
        }
        
        withUnsafePointer(to: fragmentUniform) { fragmentPointer in
            withUnsafePointer(to: vertexUniform) { vertexPointer in
                encoder.setRenderPipelineState(ViewportMTKViewDelegate.shaders[.tri]!)
                
                encoder.setVertexBuffer(vertices, offset: 0, index: 0)
                encoder.setVertexBytes(vertexPointer, length: MemoryLayout<tri_vert_uniform>.stride, index: 1);
                encoder.setFragmentBytes(fragmentPointer, length: MemoryLayout<tri_frag_uniform>.stride, index: 0);
                
                encoder.setFragmentTexture(texture, index: 0)
                
                encoder.drawPrimitives(
                    type: .triangle,
                    vertexStart: 0,
                    vertexCount: length / MemoryLayout<tri_vert_in>.stride
                )
            }
        }
        
        return true
    }
}

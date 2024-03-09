//
//  BufferManager.swift
//  Monocurl
//
//  Created by Manu Bhat on 11/1/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

import Foundation
import Metal
import MetalKit

class BufferManager {
    private var ids: Set<UInt32>
    private var map: [UInt32: (buffer: MTLBuffer?, length: Int)] = [:]
    private let gpu: MTLDevice;
    private let lock = DispatchSemaphore(value: 1)
    
    init(gpu: MTLDevice) {
        self.gpu = gpu;
        
        self.ids = Set()
        for i in 2 ..< UInt16.max {
            self.ids.insert(UInt32(i))
        }
    }
    
    public func registerID() -> UInt32 {
        lock.wait()
        
        defer {
            lock.signal()
        }
        
        guard let id = self.ids.randomElement() else {
            return 0;
        }
        
        self.ids.remove(id)
        map[id] = (nil, 0);

        return id;
    }
    
    public func buffer(for id: UInt32) -> (buffer: MTLBuffer?, length: Int)! {
        lock.wait()
        
        defer {
            lock.signal()
        }
        
        return self.map[id] 
    }
    
    public func write(bytes: UnsafeRawPointer, length: Int, into id: UInt32) {
        lock.wait()
        
        defer {
            lock.signal()
        }
        
        guard let buffer = self.map[id] else {
            NSLog("Could not find buffer!")
            return;
        }
        
        if buffer.buffer?.length ?? 0 >= length && length > 0 {
            buffer.buffer!.contents().copyMemory(from: bytes, byteCount: length);
        }
        else {
            guard let newBuffer = self.gpu.makeBuffer(length: max(1, 2 * length)) else {
                NSLog("Could not create buffer!")
                return;
            }
            
            newBuffer.contents().copyMemory(from: bytes, byteCount: length);
            self.map[id] = (newBuffer, length);            
        }
    }
    
    public func free(id: UInt32) {
        DispatchQueue.main.async {
            self.lock.wait()
            
            defer {
                self.lock.signal()
            }
            
            self.map.removeValue(forKey: id);
            
            self.ids.insert(id);
        }
    }
}

class TextureManager {
    //we could just convert it to a 1 indexed array... not sure if that would be faster
    private var map: [UInt32: MTLTexture] = [:]
    private var urlMap: [URL : MTLTexture] = [:]
    private let loader: MTKTextureLoader
    private let gpu: MTLDevice
    private let lock = DispatchSemaphore(value: 1)

    init(gpu: MTLDevice) {
        self.gpu = gpu;
        self.loader = MTKTextureLoader(device: gpu);
        
        self.map[0] = self.texture(forURL: Bundle.main.url(forResource: "1x1", withExtension: "png")!)
        self.map[1] = self.texture(forURL: Bundle.main.url(forResource: "image_not_found", withExtension: "png")!)
    }
    
    private func texture(forURL url: URL) -> MTLTexture? {
        if let cache = urlMap[url] {
            return cache;
        }
        
        var rawTexture : MTLTexture? = nil;
     
        do {
            rawTexture = try loader.newTexture(URL: url, options: [
                MTKTextureLoader.Option.SRGB : false,
//                MTKTextureLoader.Option.generateMipmaps : true
            ])
        } catch let error {
            NSLog("Failed loading texture with url '\(url)'. Error: '\(error)'");
            return nil;
        }
    
        return rawTexture;
    }
    
    public func renderTexture(width: Int, height: Int) -> (multi: MTLTexture?, single: MTLTexture?) {
        var descriptor = MTLTextureDescriptor();
        descriptor.textureType = .type2DMultisample;
        descriptor.width = width;
        descriptor.height = height;
        descriptor.pixelFormat = .bgra8Unorm;
        descriptor.usage = .renderTarget;
        descriptor.sampleCount = self.gpu.supportedSampleCount
        descriptor.storageMode = .private
        
        let multi = self.loader.device.makeTexture(descriptor: descriptor)
        
        descriptor = MTLTextureDescriptor();
        descriptor.textureType = .type2D;
        descriptor.width = width;
        descriptor.height = height;
        descriptor.pixelFormat = .bgra8Unorm;
        descriptor.usage = .shaderWrite;
        descriptor.sampleCount = 1

        let single = self.loader.device.makeTexture(descriptor: descriptor)
        
        return (multi, single)
    }
    
    public func depthTexture(width: Int, height: Int) -> MTLTexture? {
        let descriptor = MTLTextureDescriptor();
        descriptor.textureType = .type2DMultisample;
        descriptor.width = width;
        descriptor.height = height;
        descriptor.pixelFormat = .depth32Float;
        descriptor.usage = .renderTarget;
        descriptor.sampleCount = self.gpu.supportedSampleCount
        descriptor.storageMode = .private
        
        return self.loader.device.makeTexture(descriptor: descriptor)
    }
    
    //textures are always loaded, until the scene is disbanded
    public func pollID(url: URL) -> UInt32 {
        lock.wait()
        
        defer {
            lock.signal()
        }
        
        //get texture and return id
        let id = (map.keys.max() ?? 0) + 1;
        let texture = self.texture(forURL: url);
        
        map[id] = texture;
        urlMap[url] = texture;
        
        return id;
    }
    
    public func texture(for id: UInt32) -> MTLTexture? {
        /* seems to have some priority inversions issues, but I don't think there's really a way to fix this */
        lock.wait()
        
        defer {
            lock.signal()
        }
        
        //map[1] is the image not found.png
        return map[id] ?? map[1];
    }
}

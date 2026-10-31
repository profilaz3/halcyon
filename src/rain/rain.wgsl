// Original Halcyon rain kernels. Analytic droplet slopes, no ray marching,
// finite-difference reevaluation, framebuffer capture or GPU readback.
struct Params {
    frame: vec4<f32>, // aspect, time, intensity, size
    motion: vec4<f32>, // speed, wind, mist, refraction
    style: vec4<f32>, // mode, opacity, trails, unused
    crop: vec4<f32>, // visible UV origin/extent; scroll clipping keeps the image fixed
    atmosphere: vec4<f32>, // opacity, height, softness, reserved
    tint: vec4<f32>,
};
@group(0) @binding(0) var<uniform> p: Params;
@group(1) @binding(0) var background: texture_2d<f32>;
@group(1) @binding(1) var linear_sampler: sampler;
@group(1) @binding(2) var dynamic_field: texture_2d<f32>;
@group(1) @binding(3) var beads: texture_2d<f32>;
struct Vertex { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32> };
fn triangle(index:u32) -> Vertex {
    let positions=array<vec2<f32>,3>(vec2(-1.,-1.),vec2(3.,-1.),vec2(-1.,3.));
    let xy=positions[index];
    return Vertex(vec4(xy,0.,1.), vec2(xy.x*0.5+0.5,0.5-xy.y*0.5));
}
@vertex fn fullscreen(@builtin(vertex_index) index:u32) -> Vertex { return triangle(index); }
@vertex fn composite_vertex(@builtin(vertex_index) index:u32) -> Vertex {
    var v=triangle(index);
    v.uv=p.crop.xy+v.uv*p.crop.zw;
    return v;
}
fn hash(n:vec2<f32>) -> f32 {
    let h=fract(vec3(n.x,n.y,n.x)*vec3(0.1031,0.1030,0.0973));
    let q=h+dot(h,h.yzx+33.33);
    return fract((q.x+q.y)*q.z);
}
fn lens(q:vec2<f32>) -> vec4<f32> {
    let r2=dot(q,q);
    let edge=1.-smoothstep(0.72,1.,r2);
    let dome=sqrt(max(0.,1.-r2));
    return vec4(-q*dome*edge,edge,dome*edge);
}
@fragment fn static_beads(v:Vertex) -> @location(0) vec4<f32> {
    let xy=v.uv*vec2(p.frame.x,1.);
    let grid=xy*72.;let cell=floor(grid);let point=fract(grid);
    let random=hash(cell);
    if random>p.frame.z*0.6 {return vec4(0.);}
    let center=vec2(0.23+hash(cell+11.)*0.54,0.23+hash(cell+31.)*0.54);
    let radius=(0.065+hash(cell+17.)*0.085)*(0.7+p.frame.w*0.6);
    let f=lens((point-center)/radius);
    return vec4(f.xy*0.25,f.z*0.7,f.w*0.25);
}
@fragment fn procedural(v:Vertex) -> @location(0) vec4<f32> {
    let xy=v.uv*vec2(p.frame.x,1.);
    var field=vec4(0.);
    // Two independent scales break the column rhythm. Motion and shape are
    // analytic; simulation/storage cost does not grow with display resolution.
    for(var layer=0;layer<2;layer+=1) {
        let stretch=select(vec2(8.7,3.9),vec2(13.3,6.7),layer==1);
        let rate=(0.045+p.motion.x*0.17)*select(1.,0.73,layer==1);
        let offset=vec2(f32(layer)*4.37,f32(layer)*7.91);
        let q=xy*stretch+offset-vec2(p.motion.y*p.frame.y*rate*0.45,p.frame.y*rate);
        let c=floor(q);let local=fract(q);
        let random=hash(c);
        if random>p.frame.z*0.88 {continue;}
        let phase=fract(p.frame.y*0.035+random);
        let glide=smoothstep(0.55,0.95,phase)*0.32;
        let center=vec2(0.22+hash(c+3.)*0.56,0.10+hash(c+6.)*0.65+glide*0.5);
        let radius=(0.005+hash(c+12.)*0.011)*(0.5+p.frame.w)*select(1.,0.66,layer==1);
        let delta=(local-center)/stretch;
        let squash=1.15+glide*2.;
        let f=lens(delta/vec2(radius,radius*squash));
        // Individual wet traces vary in length, width and strength. Small
        // nearly stationary drops leave no trace; Trails=0 removes them all.
        let length=(0.02+hash(c+43.)*0.10)*p.style.z;
        let width=radius*(0.12+hash(c+51.)*0.16);
        let trail=(1.-smoothstep(width*0.25,width,abs(delta.x)))
            *smoothstep(-max(length,0.001),0.,delta.y)
            *(1.-smoothstep(-radius*0.4,0.,delta.y))
            *p.style.z*smoothstep(0.12,0.30,glide);
        field+=vec4(f.xy, f.z, f.w+trail*0.35);
    }
    return field;
}
struct DropVertex {
    @builtin(position) position:vec4<f32>,
    @location(0) q:vec2<f32>,
    @location(1) opacity:f32,
};
@vertex fn drop_vertex(@builtin(vertex_index) index:u32,@location(0) drop:vec4<f32>,@location(1) opacity:f32) -> DropVertex {
    let corners=array<vec2<f32>,6>(vec2(-1.,-1.),vec2(1.,-1.),vec2(-1.,1.),vec2(-1.,1.),vec2(1.,-1.),vec2(1.,1.));
    let q=corners[index];
    let xy=(drop.xy+q*vec2(drop.z,drop.z*drop.w))/vec2(p.frame.x,1.);
    return DropVertex(vec4(xy.x*2.-1.,1.-xy.y*2.,0.,1.),q,opacity);
}
@fragment fn drop_fragment(v:DropVertex)->@location(0) vec4<f32> { return lens(v.q)*v.opacity; }
@fragment fn composite(v:Vertex)->@location(0) vec4<f32> {
    if p.style.x >= 2. {
        let clean=textureSampleLevel(background,linear_sampler,v.uv,0.).rgb;
        let effect=textureSampleLevel(dynamic_field,linear_sampler,v.uv,0.);
        return vec4(mix(clean,p.tint.rgb,clamp(effect.r,0.,1.)),1.);
    }
    let small=textureSample(beads,linear_sampler,v.uv);
    let live=textureSample(dynamic_field,linear_sampler,v.uv);
    let field=small+live;
    let mask=clamp(field.z,0.,1.);
    let normal=clamp(field.xy,vec2(-1.),vec2(1.));
    let refract=normal*vec2(1./p.frame.x,1.)*(0.004+p.motion.w*0.025);
    let uv=clamp(v.uv+refract,vec2(0.001),vec2(0.999));
    let clean=textureSampleLevel(background,linear_sampler,uv,0.).rgb;
    let soft=textureSampleLevel(background,linear_sampler,uv,4.).rgb;
    let fog=p.motion.z*(1.-mask*0.85)*(1.-clamp(field.w,0.,1.)*0.5);
    var rgb=mix(clean,soft,fog);
    // Modest highlights and dark lower rims let the actual wallpaper light
    // the lens. No tint, vignette, flashes or fake scene lights are imposed.
    let shine=pow(max(0.,dot(normal,normalize(vec2(-0.6,-0.8))))*1.9,4.)*0.16*mask;
    rgb=rgb*(1.-mask*0.10)+vec3(shine);
    return vec4(rgb,p.style.y);
}
// Downsampling is done only when the wallpaper changes, into cached mipmaps.
@fragment fn downsample(v:Vertex)->@location(0) vec4<f32> {
    return textureSampleLevel(background,linear_sampler,v.uv,0.);
}

// Three analytic snow layers. Fixed work per pixel, independent of flake count.
fn snow(uv:vec2<f32>)->f32 {
    var alpha=0.;
    for(var layer=0;layer<3;layer+=1) {
        let depth=f32(layer);
        let scale=22.+depth*18.;
        let speed=p.motion.x*0.14/(1.+depth*0.55);
        let t=p.frame.y*speed;
        let drift=sin(p.frame.y*p.motion.x*0.8+depth*2.7)*0.12;
        let q=uv*vec2(p.frame.x,1.)*scale-vec2(t*p.motion.y*scale*0.6+drift,t*scale)+depth*17.3;
        let cell=floor(q);let local=fract(q);
        if hash(cell+depth*61.)>p.frame.z*0.8 {continue;}
        let flutter=sin(p.frame.y*p.motion.x*(0.6+hash(cell+83.))+hash(cell+97.)*6.283)*0.10;
        let center=vec2(0.35+hash(cell+5.)*0.3+flutter,0.25+hash(cell+19.)*0.5);
        let radius=(0.025+hash(cell+37.)*0.065)*(0.45+p.frame.w*1.5);
        let distance=length((local-center)/vec2(radius,radius*(0.85+hash(cell+71.)*0.3)));
        let flake=(1.-smoothstep(0.25,1.,distance))*(0.85-depth*0.22);
        alpha+=flake;
    }
    return clamp(alpha,0.,1.)*p.atmosphere.x;
}
@fragment fn atmosphere(v:Vertex)->@location(0) f32 {
    if p.style.x==2. {return snow(v.uv);}
    if p.style.x==3. {return fog(v.uv);}
    if p.style.x==4. {return cloud_shadows(v.uv);}
    return 0.;
}

// Smooth value noise, three bounded octaves; no evolving textures or readback.
fn noise(q:vec2<f32>)->f32 {
    let cell=floor(q);let f=fract(q);let u=f*f*(3.-2.*f);
    return mix(mix(hash(cell),hash(cell+vec2(1.,0.)),u.x),mix(hash(cell+vec2(0.,1.)),hash(cell+vec2(1.,1.)),u.x),u.y);
}
fn billows(q:vec2<f32>)->f32 {
    return noise(q)*0.57+noise(q*2.03+vec2(13.7,7.1))*0.29+noise(q*4.11+vec2(37.9,19.3))*0.14;
}
fn fog(uv:vec2<f32>)->f32 {
    if p.atmosphere.x<=0. || p.atmosphere.y<=0. {return 0.;}
    let t=p.frame.y*p.motion.x*0.08;
    let q=uv*vec2(p.frame.x*2.4,5.8)-vec2(t*(0.15+p.motion.y*1.4),t*0.12);
    let body=billows(q+vec2(noise(q*0.5)*0.8,0.));
    let top=1.-p.atmosphere.y;
    let height=smoothstep(top,min(1.,top+0.35),uv.y);
    return height*p.atmosphere.x*(0.22+smoothstep(0.20,0.85,body)*0.78);
}

fn cloud_shadows(uv:vec2<f32>)->f32 {
    if p.frame.z<=0. || p.atmosphere.x<=0. {return 0.;}
    let t=p.frame.y*p.motion.x*0.06;
    let q=uv*vec2(p.frame.x*2.1,2.7)-vec2(t*(0.2+p.motion.y),t*0.15);
    let broad=noise(q);
    let detail=billows(q);
    let body=mix(detail,broad,p.atmosphere.z*0.7);
    let edge=0.025+p.atmosphere.z*0.28;
    let threshold=1.-p.frame.z;
    let cover=smoothstep(threshold-edge,threshold+edge,body);
    return cover*p.atmosphere.x*0.75;
}

"""Original studio-control assets. Run with Blender 5: blender -b -P render.py.
No source photography or third-party textures. Camera, geometry and area lights
are fixed so widgets share the same illumination. Output has real alpha.
"""
import bpy, math
from pathlib import Path
from mathutils import Vector
ROOT = Path(__file__).resolve().parent
bpy.ops.object.select_all(action='SELECT')
bpy.ops.object.delete(use_global=False)
scene=bpy.context.scene
scene.render.engine='CYCLES'
scene.cycles.samples=64
scene.cycles.use_denoising=True
scene.render.resolution_x=512
scene.render.resolution_y=512
scene.render.resolution_percentage=100
scene.render.film_transparent=True
scene.render.image_settings.file_format='PNG'
scene.render.image_settings.color_mode='RGBA'
scene.world=bpy.data.worlds.new('Studio environment')
scene.world.use_nodes=True
scene.world.node_tree.nodes['Background'].inputs['Color'].default_value=(.12,.12,.12,1)
scene.world.node_tree.nodes['Background'].inputs['Strength'].default_value=.35
scene.view_settings.view_transform='AgX'

def material(name, color, metal=0, rough=.4):
    m=bpy.data.materials.new(name);m.use_nodes=True
    s=m.node_tree.nodes.get('Principled BSDF')
    s.inputs['Base Color'].default_value=(*color,1)
    s.inputs['Metallic'].default_value=metal
    s.inputs['Roughness'].default_value=rough
    anisotropy=s.inputs.get('Anisotropic') or s.inputs.get('Anisotropic IOR Level')
    if metal and anisotropy is not None: anisotropy.default_value=.5
    return m
rubber=material('Charcoal moulded elastomer',(.025,.03,.035),.1,.52)
cap=material('Turned aluminium',(.5,.51,.5),.85,.24)
rim=material('Black anodised aluminium',(.08,.09,.1),.8,.28)
# Fine actual surface relief, only at a photographic scale.
for mat,scale,strength in [(rubber,180,.12),(cap,260,.05)]:
    nodes=mat.node_tree.nodes; links=mat.node_tree.links
    noise=nodes.new('ShaderNodeTexNoise'); noise.inputs['Scale'].default_value=scale
    bump=nodes.new('ShaderNodeBump');bump.inputs['Strength'].default_value=strength;bump.inputs['Distance'].default_value=.025
    links.new(noise.outputs['Fac'],bump.inputs['Height']);links.new(bump.outputs['Normal'],nodes.get('Principled BSDF').inputs['Normal'])

def cylinder(name,radius,depth,z,mat,vertices=128,bevel=.03):
    bpy.ops.mesh.primitive_cylinder_add(vertices=vertices,radius=radius,depth=depth,location=(0,0,z))
    o=bpy.context.object;o.name=name;o.data.materials.append(mat)
    b=o.modifiers.new('Machined edge','BEVEL');b.width=bevel;b.segments=4
    o.modifiers.new('Weighted normals','WEIGHTED_NORMAL')
    for p in o.data.polygons:p.use_smooth=True
    return o

def light(name,location,energy,size,shape='DISK'):
    bpy.ops.object.light_add(type='AREA',location=location)
    o=bpy.context.object;o.name=name;o.data.energy=energy;o.data.shape=shape;o.data.size=size
    o.rotation_euler=(Vector((0,0,.2))-o.location).to_track_quat('-Z','Y').to_euler()
light('Large north-west softbox',(-3,4,6),550,4)
light('Cool studio fill',(4,-1,4),180,3)
light('Rear edge',(0,-4,3),100,2)
bpy.ops.object.camera_add(location=(0,-.4,7))
camera=bpy.context.object;camera.rotation_euler=(Vector((0,0,.1))-camera.location).to_track_quat('-Z','Y').to_euler()
camera.data.type='ORTHO';camera.data.ortho_scale=2.85;scene.camera=camera
objects=[]
objects.append(cylinder('Mounting bezel',1.05,.12,.06,rim,bevel=.045))
objects.append(cylinder('Elastomer grip',.96,.36,.27,rubber,bevel=.075))
for i in range(64):
    a=i/64*math.tau
    o=cylinder('Grip flute',.027,.25,.29,rubber,vertices=12,bevel=.012)
    o.location.x=.954*math.cos(a);o.location.y=.954*math.sin(a)
    objects.append(o)
objects.append(cylinder('Turned metal lip',.823,.09,.471,cap,bevel=.025))
objects.append(cylinder('Inset cap',.777,.045,.522,cap,bevel=.022))
scene.render.filepath=str(ROOT/'rotary.png');bpy.ops.render.render(write_still=True)
for o in objects:bpy.data.objects.remove(o,do_unlink=True)

def pad(name,pressed):
    bpy.ops.mesh.primitive_cube_add(size=1,location=(0,0,.12 if pressed else .18))
    o=bpy.context.object;o.name=name;o.dimensions=(2.3,1.35,.24 if pressed else .36)
    bpy.ops.object.transform_apply(location=False,rotation=False,scale=True)
    o.data.materials.append(rubber)
    bevel=o.modifiers.new('Rounded moulding','BEVEL');bevel.width=.13;bevel.segments=8
    o.modifiers.new('Surface normals','WEIGHTED_NORMAL')
    for p in o.data.polygons:p.use_smooth=True
    return o
camera.location=(0,-.65,7);camera.rotation_euler=(Vector((0,0,0))-camera.location).to_track_quat('-Z','Y').to_euler()
camera.data.ortho_scale=2.75
scene.render.resolution_x=512;scene.render.resolution_y=320
for pressed,name in [(False,'pad-up.png'),(True,'pad-down.png')]:
    o=pad('Silicone performance pad',pressed)
    scene.render.filepath=str(ROOT/name);bpy.ops.render.render(write_still=True)
    bpy.data.objects.remove(o,do_unlink=True)
print('Rendered rotary.png, pad-up.png, pad-down.png')
